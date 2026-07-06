use crate::crawler;
use crate::logs::LogManager;
use crate::models::*;
use crate::plugins::PluginEngine;
use crate::services::ServiceManager;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;

pub struct AppState {
    pub plugin_engine: Mutex<PluginEngine>,
    pub log_manager: Arc<LogManager>,
    pub service_manager: Arc<ServiceManager>,
}

// ── Tauri commands (called from frontend) ─────────────────────

#[tauri::command]
pub async fn fetch_url_cmd(request: CrawlRequest) -> CrawlResult {
    crawler::fetch_url(request).await
}

#[tauri::command]
pub async fn batch_crawl_cmd(urls: Vec<String>, rules: Vec<ExtractRule>) -> Vec<CrawlResult> {
    crawler::batch_crawl(urls, rules).await
}

#[tauri::command]
pub async fn extract_html_cmd(html: String, rules: Vec<ExtractRule>) -> Vec<ExtractedField> {
    crawler::extract_from_html(&html, &rules)
}

#[tauri::command]
pub fn execute_processor_cmd(state: State<'_, AppState>, request: ProcessRequest) -> ProcessResult {
    let mut engine = state.plugin_engine.lock().unwrap();
    engine.execute_processor(&request.processor_type, request.data, request.config)
}

#[tauri::command]
pub fn list_plugins_cmd(state: State<'_, AppState>) -> Vec<PluginInfo> {
    let engine = state.plugin_engine.lock().unwrap();
    engine.list_plugins()
}

#[tauri::command]
pub fn execute_batch_processor_cmd(
    state: State<'_, AppState>,
    pipeline: Vec<ProcessRequest>,
) -> Vec<ProcessResult> {
    let mut engine = state.plugin_engine.lock().unwrap();
    let mut results = Vec::new();

    for step in pipeline {
        let result = engine.execute_processor(&step.processor_type, step.data, step.config);
        results.push(result);
    }

    results
}

// ── RSS fetch (shared with Python bindings) ─────────────────────

pub fn inner_fetch_rss(url: &str, max_items: usize) -> Result<Vec<serde_json::Value>, String> {
    let crawl_req = CrawlRequest {
        url: url.to_string(),
        method: None,
        headers: None,
        body: None,
        use_browser: None,
        wait_for_selector: None,
        extract_rules: None,
        client_profile: None,
    };

    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let result = rt.block_on(crawler::fetch_url(crawl_req));
    let html = result.html.ok_or("No response body")?;

    let document = scraper::Html::parse_document(&html);
    let item_selector = scraper::Selector::parse("item, entry").map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for (i, element) in document.select(&item_selector).enumerate() {
        if i >= max_items {
            break;
        }

        let get_text = |tag: &str| -> String {
            let sel = scraper::Selector::parse(tag);
            if let Ok(s) = sel {
                element
                    .select(&s)
                    .next()
                    .map(|e| e.text().collect::<Vec<_>>().join(" ").trim().to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            }
        };

        let mut item = serde_json::Map::new();
        item.insert("title".into(), serde_json::Value::String(get_text("title")));
        item.insert("link".into(), serde_json::Value::String(get_text("link")));
        item.insert(
            "description".into(),
            serde_json::Value::String(get_text("description")),
        );
        item.insert(
            "pubDate".into(),
            serde_json::Value::String(get_text("pubDate, published, updated")),
        );
        item.insert(
            "author".into(),
            serde_json::Value::String(get_text("author")),
        );
        item.insert(
            "guid".into(),
            serde_json::Value::String(get_text("guid, id")),
        );

        items.push(serde_json::Value::Object(item));
    }

    Ok(items)
}

#[tauri::command]
pub async fn fetch_rss_cmd(request: RssFetchRequest) -> Result<Vec<serde_json::Value>, String> {
    let max = request.max_items.unwrap_or(50);
    inner_fetch_rss(&request.feed_url, max)
}

// ── CSV export (shared with Python bindings) ─────────────────────

pub fn inner_export_csv(data: &[serde_json::Value], delimiter: &str) -> String {
    let mut output = String::new();

    if data.is_empty() {
        return output;
    }

    let headers: Vec<String> = if let Some(first) = data.first() {
        if let Some(obj) = first.as_object() {
            obj.keys().cloned().collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    if !headers.is_empty() {
        let header_line: Vec<String> = headers
            .iter()
            .map(|h| format!("\"{}\"", h.replace('"', "\"\"")))
            .collect();
        output.push_str(&header_line.join(delimiter));
        output.push('\n');
    }

    for item in data {
        let row: Vec<String> = headers
            .iter()
            .map(|h| {
                let val = item
                    .get(h)
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();
                format!("\"{}\"", val.replace('"', "\"\""))
            })
            .collect();
        output.push_str(&row.join(delimiter));
        output.push('\n');
    }

    output
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[tauri::command]
pub async fn export_csv_cmd(request: ExportRequest) -> ExportResult {
    let delimiter = request
        .config
        .get("delimiter")
        .and_then(|v| v.as_str())
        .unwrap_or(",")
        .to_string();
    let _include_header = request
        .config
        .get("includeHeader")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let content = inner_export_csv(&request.data, &delimiter);

    ExportResult {
        file_name: format!("export_{}.csv", chrono_now()),
        mime_type: "text/csv".into(),
        content,
    }
}

// ── Excel export (Rust-side xlsx generation) ──────────────────────

pub fn inner_export_excel(data: &[serde_json::Value], sheet_name: &str) -> Result<Vec<u8>, String> {
    let wb = crate::spreadsheet::Workbook::from_json_rows(data, sheet_name);
    crate::spreadsheet::to_xlsx_bytes(&wb)
}

#[tauri::command]
pub async fn export_excel_cmd(request: ExportRequest) -> ExportResult {
    let sheet_name = request
        .config
        .get("sheetName")
        .and_then(|v| v.as_str())
        .unwrap_or("Sheet1");

    match inner_export_excel(&request.data, sheet_name) {
        Ok(bytes) => {
            use base64::Engine;
            let content = base64::engine::general_purpose::STANDARD.encode(&bytes);
            ExportResult {
                file_name: format!("export_{}.xlsx", chrono_now()),
                mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                    .into(),
                content,
            }
        }
        Err(e) => ExportResult {
            file_name: "error.txt".into(),
            mime_type: "text/plain".into(),
            content: format!("Excel export failed: {}", e),
        },
    }
}

// ── Spreadsheet commands (multi-format) ──────────────────────────

#[derive(Debug, serde::Serialize)]
pub struct SpreadsheetResult {
    pub file_name: String,
    pub mime_type: String,
    pub content: String,
}

#[tauri::command]
pub async fn spreadsheet_read_cmd(path: String) -> Result<String, String> {
    let wb = crate::spreadsheet::read(&path)?;
    serde_json::to_string(&wb).map_err(|e| format!("Serialization error: {}", e))
}

#[tauri::command]
pub async fn spreadsheet_write_cmd(data: String, path: String) -> Result<(), String> {
    let wb: crate::spreadsheet::Workbook =
        serde_json::from_str(&data).map_err(|e| format!("Invalid workbook JSON: {}", e))?;
    crate::spreadsheet::write(&wb, &path)
}

#[tauri::command]
pub async fn spreadsheet_export_cmd(
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> SpreadsheetResult {
    let format = config
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("xlsx")
        .to_lowercase();
    let sheet_name = config
        .get("sheetName")
        .and_then(|v| v.as_str())
        .unwrap_or("Sheet1");

    let content: (String, String, String) = match format.as_str() {
        "csv" => {
            let wb = crate::spreadsheet::Workbook::from_json_rows(&data, sheet_name);
            match crate::spreadsheet::to_csv_string(&wb) {
                Ok(csv) => (csv, "text/csv".into(), "csv".into()),
                Err(e) => {
                    return SpreadsheetResult {
                        file_name: "error.txt".into(),
                        mime_type: "text/plain".into(),
                        content: format!("CSV export failed: {}", e),
                    }
                }
            }
        }
        "ods" => {
            let wb = crate::spreadsheet::Workbook::from_json_rows(&data, sheet_name);
            match crate::spreadsheet::to_ods_bytes(&wb) {
                Ok(bytes) => {
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    (
                        encoded,
                        "application/vnd.oasis.opendocument.spreadsheet".into(),
                        "ods".into(),
                    )
                }
                Err(e) => {
                    return SpreadsheetResult {
                        file_name: "error.txt".into(),
                        mime_type: "text/plain".into(),
                        content: format!("ODS export failed: {}", e),
                    }
                }
            }
        }
        _ => {
            // Default: xlsx
            let wb = crate::spreadsheet::Workbook::from_json_rows(&data, sheet_name);
            match crate::spreadsheet::to_xlsx_bytes(&wb) {
                Ok(bytes) => {
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    (
                        encoded,
                        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into(),
                        "xlsx".into(),
                    )
                }
                Err(e) => {
                    return SpreadsheetResult {
                        file_name: "error.txt".into(),
                        mime_type: "text/plain".into(),
                        content: format!("XLSX export failed: {}", e),
                    }
                }
            }
        }
    };

    SpreadsheetResult {
        file_name: format!("export_{}.{}", chrono_now(), content.2),
        mime_type: content.1,
        content: content.0,
    }
}

pub fn inner_parse_html_table(
    html: &str,
    table_index: usize,
    has_header: bool,
) -> Vec<serde_json::Value> {
    let document = scraper::Html::parse_document(html);
    let table_selector = scraper::Selector::parse("table").unwrap();
    let tables: Vec<_> = document.select(&table_selector).collect();

    if table_index >= tables.len() {
        return vec![];
    }

    let table = tables[table_index];
    let row_selector = scraper::Selector::parse("tr").unwrap();
    let cell_selector = scraper::Selector::parse("th, td").unwrap();

    let rows: Vec<_> = table.select(&row_selector).collect();
    let mut result = Vec::new();

    let headers: Vec<String> = if has_header && !rows.is_empty() {
        rows[0]
            .select(&cell_selector)
            .map(|c| c.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .collect()
    } else {
        (0..rows
            .first()
            .map(|r| r.select(&cell_selector).count())
            .unwrap_or(0))
            .map(|i| format!("col_{}", i))
            .collect()
    };

    let start_row = if has_header { 1 } else { 0 };
    for i in start_row..rows.len() {
        let cells: Vec<_> = rows[i].select(&cell_selector).collect();
        let mut item = serde_json::Map::new();
        for (j, cell) in cells.iter().enumerate() {
            if j < headers.len() {
                let text = cell.text().collect::<Vec<_>>().join(" ").trim().to_string();
                item.insert(headers[j].clone(), serde_json::Value::String(text));
            }
        }
        if !item.is_empty() {
            result.push(serde_json::Value::Object(item));
        }
    }

    result
}

#[tauri::command]
pub async fn parse_html_table_cmd(
    html: String,
    config: serde_json::Value,
) -> Vec<serde_json::Value> {
    let table_index = config
        .get("tableIndex")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let has_header = config
        .get("hasHeader")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    inner_parse_html_table(&html, table_index, has_header)
}

// ── Python plugin commands ───────────────────────────────────────

#[tauri::command]
pub fn list_python_plugins_cmd(
    state: State<'_, AppState>,
) -> Vec<crate::python_plugins::PythonPluginMeta> {
    let mut engine = state.plugin_engine.lock().unwrap();
    engine.list_python_plugins_meta()
}

#[tauri::command]
pub fn execute_python_hook_cmd(
    state: State<'_, AppState>,
    plugin_id: String,
    hook_name: String,
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> Result<Vec<serde_json::Value>, String> {
    let mut engine = state.plugin_engine.lock().unwrap();
    engine.call_python_hook(&plugin_id, &hook_name, data, config)
}

#[tauri::command]
pub fn call_python_data_source_cmd(
    state: State<'_, AppState>,
    plugin_id: String,
    config: serde_json::Value,
) -> Result<Vec<serde_json::Value>, String> {
    let mut engine = state.plugin_engine.lock().unwrap();
    engine.call_python_data_source(&plugin_id, config)
}

#[tauri::command]
pub fn call_python_export_cmd(
    state: State<'_, AppState>,
    plugin_id: String,
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> Result<String, String> {
    let mut engine = state.plugin_engine.lock().unwrap();
    engine.call_python_export(&plugin_id, data, config)
}

#[tauri::command]
pub fn run_python_pipeline_cmd(
    state: State<'_, AppState>,
    steps: Vec<crate::python_plugins::PipelineStep>,
    initial_data: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut engine = state.plugin_engine.lock().unwrap();
    engine.run_python_pipeline(steps, initial_data)
}

#[tauri::command]
pub fn reload_python_plugins_cmd(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let mut engine = state.plugin_engine.lock().unwrap();
    engine.reload_python_plugins()
}

// ── BeautifulSoup HTML parse: Python → Rust struct ──────────────

/// Parse HTML using the BeautifulSoup Python plugin and deserialize
/// into native Rust `ParsedHtmlItem` structs.
#[tauri::command]
pub fn parse_html_with_bs4_cmd(
    state: State<'_, AppState>,
    html: String,
    config: serde_json::Value,
) -> Result<Vec<ParsedHtmlItem>, String> {
    let mut engine = state.plugin_engine.lock().unwrap();

    // Pass HTML to the bs4-parser Python plugin's process_data hook
    let input = vec![serde_json::json!({ "html": html })];
    let json_result = engine.call_python_hook("bs4-parser", "process_data", input, config)?;

    // Deserialize JSON into Rust structs
    let items: Vec<ParsedHtmlItem> = serde_json::from_value(serde_json::Value::Array(json_result))
        .map_err(|e| format!("Failed to deserialize bs4 output into Rust structs: {}", e))?;

    Ok(items)
}

/// Process parsed HTML items — demonstrates Rust-side processing of
/// data parsed by Python BeautifulSoup.
#[tauri::command]
pub fn summarize_parsed_html_cmd(items: Vec<ParsedHtmlItem>) -> ParsedHtmlSummary {
    let mut summary = ParsedHtmlSummary {
        total_items: items.len(),
        links: vec![],
        images: vec![],
        headings: vec![],
        meta_tags: vec![],
        tables: vec![],
        text_blocks: vec![],
    };

    for item in items {
        match item.item_type.as_str() {
            "link" => summary.links.push(item),
            "image" => summary.images.push(item),
            "heading" => summary.headings.push(item),
            "meta" => summary.meta_tags.push(item),
            "table" => summary.tables.push(item),
            "text" => summary.text_blocks.push(item),
            _ => summary.text_blocks.push(item),
        }
    }

    summary
}

// ── Presets ────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_presets_cmd(state: State<'_, AppState>) -> Vec<serde_json::Value> {
    let mut guard = state.plugin_engine.lock().unwrap();
    guard.list_presets()
}

// ── Demo pipeline ──────────────────────────────────────────────────

fn demo_sample_data() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"id": 1, "title": "CrawlFlow Demo", "description": "A visual web crawler configurator", "author": "CrawlFlow Team", "url": "https://crawlflow.ai", "tags": ["crawler", "visual", "tool"], "views": 1520}),
        serde_json::json!({"id": 2, "title": "Getting Started Guide", "description": "Learn how to use CrawlFlow in 5 minutes", "author": "Docs Team", "url": "https://crawlflow.ai/docs", "tags": ["guide", "tutorial"], "views": 890}),
        serde_json::json!({"id": 3, "title": "Plugin Development", "description": "Create your own Python plugins", "author": "Dev Team", "url": "https://crawlflow.ai/plugins", "tags": ["python", "plugin", "dev"], "views": 340}),
        serde_json::json!({"id": 4, "title": "Marketplace Launch", "description": "Browse and install community plugins", "author": "Community", "url": "https://crawlflow.ai/marketplace", "tags": ["marketplace", "community"], "views": 2100}),
        serde_json::json!({"id": 5, "title": "Architecture Overview", "description": "Deep dive into the CrawlFlow architecture", "author": "CrawlFlow Team", "url": "https://crawlflow.ai/architecture", "tags": ["architecture", "deep-dive"], "views": 670}),
    ]
}

#[tauri::command]
pub fn run_demo_cmd() -> Result<serde_json::Value, String> {
    use crate::plugins;

    // Step 1: Get sample data
    let data = demo_sample_data();
    let mut result = serde_json::json!({
        "step1_fetch": {
            "label": "Fetched sample data",
            "count": data.len(),
            "data": data
        }
    });

    // Step 2: Run deduplicate processor (remove duplicates by "id")
    let dedup_config = serde_json::json!({"field": "id"});
    let dedup_plugin = plugins::RustPlugin {
        id: "rust-deduplicate".into(),
        name: "Deduplicate".into(),
        version: "1.0.0".into(),
        description: "".into(),
        capabilities: vec![],
        execute: plugins::deduplicate_plugin,
    };
    let dedup_result = (dedup_plugin.execute)(data, dedup_config).map_err(|e| e)?;
    result["step2_deduplicate"] = serde_json::json!({
        "label": "Removed duplicate items by id field",
        "count": dedup_result.len(),
        "data": dedup_result
    });

    // Step 3: Run filter processor (only items with views > 500)
    let filter_config =
        serde_json::json!({"field": "views", "operator": "greater_than", "value": "500"});
    let filter_plugin = plugins::RustPlugin {
        id: "rust-filter".into(),
        name: "Filter".into(),
        version: "1.0.0".into(),
        description: "".into(),
        capabilities: vec![],
        execute: plugins::filter_plugin,
    };
    let filter_result = (filter_plugin.execute)(dedup_result, filter_config).map_err(|e| e)?;
    result["step3_filter"] = serde_json::json!({
        "label": "Filtered items with views > 500",
        "count": filter_result.len(),
        "data": filter_result
    });

    // Step 4: Run sort processor (by views descending)
    let sort_config = serde_json::json!({"field": "views", "descending": true});
    let sort_plugin = plugins::RustPlugin {
        id: "rust-sort".into(),
        name: "Sort".into(),
        version: "1.0.0".into(),
        description: "".into(),
        capabilities: vec![],
        execute: plugins::sort_plugin,
    };
    let sort_result = (sort_plugin.execute)(filter_result, sort_config).map_err(|e| e)?;
    result["step4_sort"] = serde_json::json!({
        "label": "Sorted by views (descending)",
        "count": sort_result.len(),
        "data": sort_result
    });

    // Step 5: Run limit processor (top 3)
    let limit_config = serde_json::json!({"count": 3, "offset": 0});
    let limit_plugin = plugins::RustPlugin {
        id: "rust-limit".into(),
        name: "Limit".into(),
        version: "1.0.0".into(),
        description: "".into(),
        capabilities: vec![],
        execute: plugins::limit_plugin,
    };
    let limit_result = (limit_plugin.execute)(sort_result, limit_config).map_err(|e| e)?;
    result["step5_limit"] = serde_json::json!({
        "label": "Limited to top 3 results",
        "count": limit_result.len(),
        "data": limit_result
    });

    result["final_output"] = serde_json::json!({
        "label": "Final demo output (CSV-ready)",
        "count": limit_result.len(),
        "data": limit_result
    });

    Ok(result)
}

// ── Service commands ───────────────────────────────────────────────────

#[tauri::command]
pub fn start_project_service_cmd(
    state: State<'_, AppState>,
    project_id: String,
    nodes: Vec<serde_json::Value>,
    edges: Vec<serde_json::Value>,
    settings: serde_json::Value,
) -> Result<String, String> {
    log::info!(
        "start_project_service_cmd: {} nodes, {} edges",
        nodes.len(),
        edges.len()
    );
    if let Some(first) = nodes.first() {
        log::info!(
            "start_project_service_cmd: first node keys: {:?}",
            first.as_object().map(|o| o.keys().collect::<Vec<_>>())
        );
    }
    state
        .service_manager
        .start_service(&project_id, nodes, edges, settings)
        .map(|_| format!("Service started for project {}", project_id))
}

#[tauri::command]
pub fn stop_project_service_cmd(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<String, String> {
    state
        .service_manager
        .stop_service(&project_id)
        .map(|_| format!("Service stopped for project {}", project_id))
}

#[tauri::command]
pub fn pause_project_service_cmd(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<String, String> {
    state
        .service_manager
        .pause_service(&project_id)
        .map(|_| format!("Service paused for project {}", project_id))
}

#[tauri::command]
pub fn resume_project_service_cmd(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<String, String> {
    state
        .service_manager
        .resume_service(&project_id)
        .map(|_| format!("Service resumed for project {}", project_id))
}

#[tauri::command]
pub fn get_service_status_cmd(
    state: State<'_, AppState>,
    project_id: String,
) -> Option<crate::services::ServiceInfo> {
    state.service_manager.get_service_info(&project_id)
}

#[tauri::command]
pub fn list_project_services_cmd(state: State<'_, AppState>) -> Vec<crate::services::ServiceInfo> {
    state.service_manager.list_service_infos()
}

// ── Log commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_project_logs_cmd(
    state: State<'_, AppState>,
    project_id: String,
    since_id: Option<u64>,
    level_filter: Option<String>,
    limit: Option<usize>,
) -> Vec<crate::logs::LogEntry> {
    state
        .log_manager
        .get_logs(&project_id, since_id, level_filter.as_deref(), limit)
}

#[tauri::command]
pub fn clear_project_logs_cmd(state: State<'_, AppState>, project_id: String) -> String {
    state.log_manager.clear(&project_id);
    format!("Logs cleared for project {}", project_id)
}

// ── Progress commands ────────────────────────────────────────────────

#[tauri::command]
pub fn get_project_progress_cmd(project_id: String) -> Option<crate::progress::ProgressInfo> {
    crate::progress::get_progress(&project_id)
}

// ── Request Client commands ───────────────────────────────────────────

#[tauri::command]
pub async fn fetch_with_client_cmd(
    url: String,
    profile: crate::models::ClientProfile,
    extract_rules: Option<Vec<ExtractRule>>,
) -> CrawlResult {
    crate::request_clients::fetch_with_client(&url, &profile, extract_rules).await
}

// ── System Service commands ────────────────────────────────────────────

#[tauri::command]
pub fn get_service_install_info_cmd() -> crate::system_service::ServiceInstallInfo {
    crate::system_service::SystemServiceManager::get_info()
}

#[tauri::command]
pub fn install_system_service_cmd() -> Result<String, String> {
    crate::system_service::SystemServiceManager::install()
}

#[tauri::command]
pub fn uninstall_system_service_cmd() -> Result<String, String> {
    crate::system_service::SystemServiceManager::uninstall()
}

#[tauri::command]
pub fn start_system_service_cmd() -> Result<String, String> {
    crate::system_service::SystemServiceManager::start()
}

#[tauri::command]
pub fn stop_system_service_cmd() -> Result<String, String> {
    crate::system_service::SystemServiceManager::stop()
}

// ── Marketplace installation ─────────────────────────────────────────

fn get_user_plugins_dir() -> PathBuf {
    dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("crawlflow")
}

#[tauri::command]
pub async fn install_marketplace_item(
    slug: String,
    item_type: String,
    download_url: String,
) -> Result<String, String> {
    let base_dir = if item_type == "template" {
        get_user_plugins_dir().join("templates").join(&slug)
    } else {
        get_user_plugins_dir().join("plugins").join(&slug)
    };

    std::fs::create_dir_all(&base_dir).map_err(|e| format!("Failed to create dir: {}", e))?;

    // Download the zip
    let response = reqwest::get(&download_url)
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Extract zip
    let reader = std::io::Cursor::new(&bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("Failed to open zip: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {}", e))?;

        let out_path = base_dir.join(file.name());

        if file.is_dir() {
            std::fs::create_dir_all(&out_path).ok();
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let mut outfile = std::fs::File::create(&out_path)
                .map_err(|e| format!("Failed to create file {:?}: {}", out_path, e))?;
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("Failed to write file: {}", e))?;
        }
    }

    Ok(base_dir.to_string_lossy().to_string())
}

// ── Settings Engine Commands ──────────────────────────────

#[tauri::command]
pub fn list_processor_settings_schemas(
) -> std::collections::HashMap<String, crate::settings_engine::SettingsSchema> {
    crate::settings_engine::list_processor_schemas()
}

#[tauri::command]
pub fn get_processor_settings_schema(
    processor_id: String,
) -> Option<crate::settings_engine::SettingsSchema> {
    crate::settings_engine::get_processor_schema(&processor_id)
}

#[tauri::command]
pub fn validate_settings_values(
    processor_id: String,
    values: serde_json::Value,
) -> Result<Vec<crate::settings_engine::ValidationError>, String> {
    let schema = crate::settings_engine::get_processor_schema(&processor_id)
        .ok_or_else(|| format!("Processor '{}' not found", processor_id))?;
    Ok(schema.validate(&values))
}

#[tauri::command]
pub fn get_settings_defaults(processor_id: String) -> Result<serde_json::Value, String> {
    let schema = crate::settings_engine::get_processor_schema(&processor_id)
        .ok_or_else(|| format!("Processor '{}' not found", processor_id))?;
    Ok(schema.apply_defaults())
}

// ── Raw Items Commands ────────────────────────────────────

fn get_project_db_path(project_id: &str) -> PathBuf {
    let data_dir = dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("crawlflow")
        .join("projects");
    data_dir.join(format!("project_{}.db", project_id))
}

#[tauri::command]
pub fn get_raw_items_cmd(
    project_id: String,
    status: Option<String>,
    search: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
    sort_by: Option<String>,
    sort_dir: Option<String>,
) -> Result<serde_json::Value, String> {
    let db_path = get_project_db_path(&project_id);
    if !db_path.exists() {
        return Ok(serde_json::json!({ "items": [], "total": 0 }));
    }

    let repo = crate::repository::RawItemRepository::open(&db_path)?;
    let query = crate::repository::ItemsQuery {
        status,
        worker_id: None,
        search,
        matched: None,
        limit: limit.unwrap_or(50),
        offset: offset.unwrap_or(0),
        sort_by,
        sort_dir,
    };

    let result = repo.query_items(&query)?;
    Ok(serde_json::json!({ "items": result.items, "total": result.total }))
}

#[tauri::command]
pub fn get_raw_items_summary_cmd(
    project_id: String,
) -> Result<crate::repository::ItemsSummary, String> {
    let db_path = get_project_db_path(&project_id);
    if !db_path.exists() {
        return Ok(crate::repository::ItemsSummary {
            total: 0,
            pending: 0,
            processing: 0,
            done: 0,
            error: 0,
            ignored: 0,
        });
    }

    let repo = crate::repository::RawItemRepository::open(&db_path)?;
    repo.get_summary()
}

fn master_db_path() -> PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.crawlflow.desktop")
        .join("crawlflow.db")
}

#[tauri::command]
pub fn get_app_setting_cmd(key: String) -> Result<Option<String>, String> {
    let db_path = master_db_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("Failed to open master DB: {}", e))?;
    let mut stmt = conn
        .prepare("SELECT value FROM app_settings WHERE key = ?1")
        .map_err(|e| format!("Failed to prepare: {}", e))?;
    let result: Option<String> = stmt.query_row([&key], |row| row.get(0)).ok();
    Ok(result)
}

#[tauri::command]
pub fn set_app_setting_cmd(key: String, value: String) -> Result<(), String> {
    let db_path = master_db_path();
    let conn = rusqlite::Connection::open(&db_path)
        .map_err(|e| format!("Failed to open master DB: {}", e))?;
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )
    .map_err(|e| format!("Failed to save setting: {}", e))?;
    Ok(())
}

fn get_lock_file_path(project_id: &str) -> PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.crawlflow.desktop")
        .join(format!("{}.edit", project_id))
}

#[tauri::command]
pub fn lock_project_edit_cmd(project_id: String) -> Result<(), String> {
    let db_path = master_db_path();
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS project_runtime (
            project_id TEXT PRIMARY KEY,
            runner_status TEXT NOT NULL DEFAULT 'stopped',
            runner_pid INTEGER,
            runner_type TEXT DEFAULT 'app',
            edit_pid INTEGER,
            cycle_count INTEGER NOT NULL DEFAULT 0,
            last_run_at TEXT,
            last_error TEXT,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .ok();
    let pid = std::process::id() as i64;
    conn.execute(
        "INSERT INTO project_runtime (project_id, edit_pid, runner_status, updated_at)
         VALUES (?1, ?2, 'stopped', datetime('now'))
         ON CONFLICT(project_id) DO UPDATE SET edit_pid = ?2, updated_at = datetime('now')",
        rusqlite::params![project_id, pid],
    )
    .map_err(|e| e.to_string())?;
    log::info!("Locked project {} for editing (PID: {})", project_id, pid);
    Ok(())
}

#[tauri::command]
pub fn unlock_project_edit_cmd(project_id: String) -> Result<(), String> {
    let db_path = master_db_path();
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE project_runtime SET edit_pid = NULL, updated_at = datetime('now') WHERE project_id = ?1",
        rusqlite::params![project_id],
    ).ok();
    log::info!("Unlocked project {} from editing", project_id);
    Ok(())
}
