use crate::crawler;
use crate::models::*;
use crate::plugins::PluginEngine;
use std::sync::Mutex;
use tauri::State;
use std::path::PathBuf;

pub struct AppState {
    pub plugin_engine: Mutex<PluginEngine>,
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
pub fn execute_processor_cmd(
    state: State<'_, AppState>,
    request: ProcessRequest,
) -> ProcessResult {
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
        item.insert("author".into(), serde_json::Value::String(get_text("author")));
        item.insert("guid".into(), serde_json::Value::String(get_text("guid, id")));

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

// ── HTML table parser (shared with Python bindings) ──────────────

pub fn inner_parse_html_table(html: &str, table_index: usize, has_header: bool) -> Vec<serde_json::Value> {
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
pub async fn parse_html_table_cmd(html: String, config: serde_json::Value) -> Vec<serde_json::Value> {
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
pub fn list_python_plugins_cmd(state: State<'_, AppState>) -> Vec<crate::python_plugins::PythonPluginMeta> {
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
pub fn summarize_parsed_html_cmd(
    items: Vec<ParsedHtmlItem>,
) -> ParsedHtmlSummary {
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

// ── Marketplace installation ─────────────────────────────────────────

fn get_data_dir() -> PathBuf {
    dirs_next::data_dir()
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
        get_data_dir().join("templates").join(&slug)
    } else {
        get_data_dir().join("plugins").join(&slug)
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
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| format!("Failed to open zip: {}", e))?;

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
