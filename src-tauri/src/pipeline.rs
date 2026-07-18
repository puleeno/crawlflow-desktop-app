use crate::data_preprocessor::{DataPreprocessor, PreprocessorConfig, UrlPattern};
use crate::finish_actions::ActionEngine;
use crate::logs::LogManager;
use crate::models::ClientProfile;
use crate::plugins;
use crate::python_plugins::PythonPluginEngine;
use crate::repository::RawItemRepository;
use crate::request_clients;
use crate::worker_engine::WorkerEngine;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    Queue,
    Parallel,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        ExecutionMode::Queue
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub label: Option<String>,
    pub data: serde_json::Value,
    pub position: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub source_handle: Option<String>,
    pub target_handle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub nodes: Vec<PipelineNode>,
    pub edges: Vec<PipelineEdge>,
    pub settings: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub node_id: String,
    pub node_label: String,
    pub node_type: String,
    pub input_count: usize,
    pub output_count: usize,
    pub detail: String,
    pub output: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub steps: Vec<ExecutionStep>,
    pub final_output: Vec<serde_json::Value>,
    pub error: Option<String>,
}

fn extract_client_profile(node_data: &serde_json::Value) -> ClientProfile {
    let default_timeout = Some(30u64);
    if let Some(url_settings) = node_data.get("urlSettings") {
        if let Some(http_client) = url_settings.get("httpClient") {
            let client_type = http_client
                .get("clientType")
                .and_then(|v| v.as_str())
                .unwrap_or("reqwest")
                .to_string();
            let user_agent = http_client
                .get("userAgent")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let proxy_url = http_client
                .get("proxyUrl")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let timeout_secs = http_client
                .get("timeoutSecs")
                .and_then(|v| v.as_u64())
                .or(default_timeout);
            let headers = http_client
                .get("headers")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|h| {
                            let key = h.get("key")?.as_str()?.to_string();
                            let value = h.get("value")?.as_str()?.to_string();
                            Some((key, value))
                        })
                        .collect::<Vec<_>>()
                });
            let chrome_args = http_client
                .get("chromeArgs")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| a.as_str().map(|s| s.to_string()))
                        .collect()
                });
            let wait_for_selector = http_client
                .get("waitForSelector")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());

            let headless = http_client.get("headless").and_then(|v| v.as_bool());
            return ClientProfile {
                client_type,
                user_agent,
                proxy_url,
                headers,
                timeout_secs,
                profile_dir: None,
                chrome_args,
                wait_for_selector,
                extra_nav_args: None,
                headless,
            };
        }
    }
    ClientProfile::default()
}

fn demo_sample_data() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"id": 1, "title": "CrawlFlow Demo", "description": "A visual web crawler configurator", "author": "CrawlFlow Team", "url": "https://crawlflow.ai", "tags": ["crawler", "visual", "tool"], "views": 1520}),
        serde_json::json!({"id": 2, "title": "Getting Started Guide", "description": "Learn how to use CrawlFlow in 5 minutes", "author": "Docs Team", "url": "https://crawlflow.ai/docs", "tags": ["guide", "tutorial"], "views": 890}),
        serde_json::json!({"id": 3, "title": "Plugin Development", "description": "Create your own Python plugins", "author": "Dev Team", "url": "https://crawlflow.ai/plugins", "tags": ["python", "plugin", "dev"], "views": 340}),
        serde_json::json!({"id": 4, "title": "Marketplace Launch", "description": "Browse and install community plugins", "author": "Community", "url": "https://crawlflow.ai/marketplace", "tags": ["marketplace", "community"], "views": 2100}),
        serde_json::json!({"id": 5, "title": "Architecture Overview", "description": "Deep dive into the CrawlFlow architecture", "author": "CrawlFlow Team", "url": "https://crawlflow.ai/architecture", "tags": ["architecture", "deep-dive"], "views": 670}),
    ]
}

fn topological_sort(nodes: &[PipelineNode], edges: &[PipelineEdge]) -> Result<Vec<String>, String> {
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

    for node in nodes {
        in_degree.entry(node.id.clone()).or_insert(0);
        adjacency.entry(node.id.clone()).or_default();
    }

    for edge in edges {
        if let Some(degree) = in_degree.get_mut(&edge.target) {
            *degree += 1;
        }
        adjacency
            .entry(edge.source.clone())
            .or_default()
            .push(edge.target.clone());
    }

    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut order = Vec::new();
    while let Some(node_id) = queue.pop_front() {
        order.push(node_id.clone());
        if let Some(neighbors) = adjacency.get(&node_id) {
            for neighbor in neighbors {
                if let Some(degree) = in_degree.get_mut(neighbor) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    if order.len() != nodes.len() {
        return Err(format!(
            "Cycle detected: processed {} of {} nodes",
            order.len(),
            nodes.len()
        ));
    }

    Ok(order)
}

/// Group topologically-sorted nodes by depth level.
/// Nodes at the same level have no dependencies on each other and can run in parallel.
fn topological_levels(order: &[String], edges: &[PipelineEdge]) -> Vec<Vec<String>> {
    let mut depth: HashMap<String, usize> = HashMap::new();
    for node_id in order {
        let mut max_depth = 0usize;
        for edge in edges {
            if edge.target == *node_id {
                if let Some(d) = depth.get(&edge.source) {
                    max_depth = max_depth.max(d + 1);
                }
            }
        }
        depth.insert(node_id.clone(), max_depth);
    }

    let max_level = depth.values().copied().max().unwrap_or(0);
    let mut levels: Vec<Vec<String>> = vec![vec![]; max_level + 1];
    for node_id in order {
        let d = depth.get(node_id).copied().unwrap_or(0);
        levels[d].push(node_id.clone());
    }
    levels
}

#[allow(dead_code)]
fn node_inputs(
    node_id: &str,
    edges: &[PipelineEdge],
    node_outputs: &HashMap<String, Vec<serde_json::Value>>,
) -> Vec<serde_json::Value> {
    let mut combined = Vec::new();
    for edge in edges {
        if edge.target == node_id {
            if let Some(output) = node_outputs.get(&edge.source) {
                combined.extend(output.iter().cloned());
            }
        }
    }
    combined
}

/// Dữ liệu thô đã fetch (HTML/RAW) dùng trong Phase 1 của repository pipeline.
pub struct FetchedData {
    pub source_url: String,
    pub raw_data: String,
    pub input_type: String,
    pub chrome_session: Option<crate::models::ChromeSession>,
    /// True when this entry is a listing URL already rewritten by a
    /// Python preprocessor (e.g. Oreka storeId rewrite). In Phase 1b
    /// Stage A we must NOT rewrite it again — just fetch its HTML and
    /// feed it to Stage B for product-URL extraction.
    pub from_plugin_listing: bool,
}

/// Lightweight placeholder for listing URLs produced by a Python
/// preprocessor/data-source hook. `is_listing_url=true` means the
/// item's `source_url` is a rewritten listing URL that Phase 1b Stage A
/// should fetch and feed into the listing_pages pipeline.
pub struct FetchedDataPlaceholder {
    pub source_url: String,
    pub raw_data: String,
    pub input_type: String,
    pub is_listing_url: bool,
}

pub fn execute_pipeline(
    config: &PipelineConfig,
    log_manager: &Arc<LogManager>,
    project_id: &str,
) -> ExecutionResult {
    execute_pipeline_with_mode(config, ExecutionMode::Queue, log_manager, project_id)
}

pub fn execute_pipeline_with_mode(
    config: &PipelineConfig,
    mode: ExecutionMode,
    log_manager: &Arc<LogManager>,
    project_id: &str,
) -> ExecutionResult {
    let mut steps = Vec::new();
    let node_outputs: Arc<RwLock<HashMap<String, Vec<serde_json::Value>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // Route Python plugin logs (crawlflow.log) to this project's LogManager.
    let _log_guard = crate::logs::LogContextGuard::new(log_manager.clone(), project_id);

    let order = match topological_sort(&config.nodes, &config.edges) {
        Ok(o) => o,
        Err(e) => {
            return ExecutionResult {
                success: false,
                steps: vec![],
                final_output: vec![],
                error: Some(e),
            };
        }
    };

    log_manager.info(
        project_id,
        "pipeline",
        &format!(
            "Pipeline execution order: {} nodes (mode: {:?})",
            order.len(),
            mode
        ),
    );

    let levels = topological_levels(&order, &config.edges);

    for level_nodes in &levels {
        match mode {
            ExecutionMode::Queue => {
                for node_id in level_nodes {
                    process_node(
                        node_id,
                        config,
                        &node_outputs,
                        log_manager,
                        project_id,
                        &mut steps,
                    );
                }
            }
            ExecutionMode::Parallel => {
                let mut handles = Vec::new();
                for node_id in level_nodes {
                    let config = config.clone();
                    let outputs = node_outputs.clone();
                    let lm = log_manager.clone();
                    let pid = project_id.to_string();
                    let nid = node_id.clone();

                    handles.push(std::thread::spawn(move || {
                        let mut local_steps = Vec::new();
                        process_node(&nid, &config, &outputs, &lm, &pid, &mut local_steps);
                        local_steps
                    }));
                }

                for handle in handles {
                    if let Ok(mut s) = handle.join() {
                        steps.append(&mut s);
                    }
                }
            }
        }
    }

    let final_output = if let Some(last) = order.last() {
        node_outputs
            .read()
            .unwrap()
            .get(last)
            .cloned()
            .unwrap_or_default()
    } else {
        vec![]
    };

    log_manager.info(
        project_id,
        "pipeline",
        &format!(
            "Pipeline complete: {} steps, final output: {} items",
            steps.len(),
            final_output.len()
        ),
    );

    ExecutionResult {
        success: true,
        steps,
        final_output,
        error: None,
    }
}

fn process_node(
    node_id: &str,
    config: &PipelineConfig,
    node_outputs: &Arc<RwLock<HashMap<String, Vec<serde_json::Value>>>>,
    log_manager: &Arc<LogManager>,
    project_id: &str,
    steps: &mut Vec<ExecutionStep>,
) {
    let node = match config.nodes.iter().find(|n| n.id == node_id) {
        Some(n) => n,
        None => return,
    };

    let input = node_inputs_read(node_id, &config.edges, node_outputs);
    let input_count = input.len();

    let (output, detail) = match node.node_type.as_str() {
        "start" | "dataSource" | "rssSource" => {
            let items = demo_sample_data();
            let count = items.len();
            log_manager.info(
                project_id,
                node_id,
                &format!("[{}] Generated {} sample data items", label_of(node), count),
            );
            (items, format!("Generated {} sample items", count))
        }

        "repository" => {
            let count = input.len();
            log_manager.info(
                project_id,
                node_id,
                &format!("[{}] Stored {} items in repository", label_of(node), count),
            );
            (input, format!("Stored {} items", count))
        }

        // The fetchData node is a structural step that makes the data-fetching phase
        // explicit in the flow diagram. The actual HTTP/file fetch is performed by
        // the repository pipeline (Phase 1a). Here we simply pass items through.
        "fetchData" => {
            let count = input.len();
            log_manager.info(
                project_id,
                node_id,
                &format!(
                    "[{}] Fetch / Get Data: forwarding {} items to repository",
                    label_of(node),
                    count
                ),
            );
            (input, format!("Fetched & forwarded {} items", count))
        }

        "processor" => {
            let processor_type = node
                .data
                .get("processorType")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let processor_config = node
                .data
                .get("processorConfig")
                .cloned()
                .or_else(|| node.data.get("settings").cloned())
                .or_else(|| node.data.get("config").cloned())
                .unwrap_or(serde_json::Value::Null);

            // Inject `extractFields` (field names from the upstream extractor
            // node) so the export plugin keeps the extractor output columns
            // instead of dropping them as metadata.
            let mut processor_config_obj = if processor_config.is_object() {
                processor_config.as_object().cloned().unwrap()
            } else {
                serde_json::Map::new()
            };
            if !processor_config_obj.contains_key("extractFields") {
                let extract_fields: Vec<String> = config
                    .nodes
                    .iter()
                    .filter(|n| n.node_type == "html-data-extractor")
                    .filter_map(|n| {
                        let rules = n
                            .data
                            .get("customRules")
                            .or_else(|| n.data.get("extractionRules"))
                            .or_else(|| n.data.get("extractRules"))?
                            .as_array()?;
                        Some(
                            rules
                                .iter()
                                .filter_map(|r| {
                                    r.get("name")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string())
                                })
                                .collect::<Vec<String>>(),
                        )
                    })
                    .flatten()
                    .collect();
                if !extract_fields.is_empty() {
                    processor_config_obj
                        .insert("extractFields".into(), serde_json::json!(extract_fields));
                }
            }
            let processor_config = serde_json::Value::Object(processor_config_obj);

            let result = match processor_type {
                "rust-deduplicate" => {
                    let cfg = processor_config;
                    plugins::deduplicate_plugin(input, cfg)
                }
                "rust-filter" => {
                    let cfg = processor_config;
                    plugins::filter_plugin(input, cfg)
                }
                "rust-sort" => {
                    let cfg = processor_config;
                    plugins::sort_plugin(input, cfg)
                }
                "rust-limit" => {
                    let cfg = processor_config;
                    plugins::limit_plugin(input, cfg)
                }
                "rust-excel-export" | "excel-export" => {
                    log_manager.info(
                        project_id,
                        node_id,
                        &format!(
                            "[{}] Excel export starting: {} items, config={}",
                            label_of(node),
                            input_count,
                            processor_config
                        ),
                    );
                    let cfg = processor_config.clone();
                    match plugins::excel_export_plugin(input.clone(), cfg) {
                        Ok(output) => {
                            let file = output
                                .first()
                                .and_then(|v| v.get("file"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("<unknown>");
                            log_manager.info(
                                project_id,
                                node_id,
                                &format!("[{}] Excel export wrote file: {}", label_of(node), file),
                            );
                            Ok(output)
                        }
                        Err(e) => Err(e),
                    }
                }
                _ => {
                    log_manager.warn(
                        project_id,
                        node_id,
                        &format!("Unknown processor type: {}", processor_type),
                    );
                    Ok(input)
                }
            };

            match result {
                Ok(output) => {
                    let out_count = output.len();
                    log_manager.info(
                        project_id,
                        node_id,
                        &format!(
                            "[{}] {}: {} → {} items",
                            label_of(node),
                            processor_type,
                            input_count,
                            out_count
                        ),
                    );
                    (
                        output,
                        format!("{}: {} → {} items", processor_type, input_count, out_count),
                    )
                }
                Err(e) => {
                    log_manager.error(
                        project_id,
                        node_id,
                        &format!("[{}] Processor failed: {}", label_of(node), e),
                    );
                    (vec![], format!("Error: {}", e))
                }
            }
        }

        "htmlExtractor" => {
            log_manager.info(
                project_id,
                node_id,
                &format!(
                    "[{}] Passed through {} items (extraction not yet implemented in service)",
                    label_of(node),
                    input_count
                ),
            );
            (
                input,
                format!("Passed through {} items (extraction stubbed)", input_count),
            )
        }

        "excelExport" => {
            let count = input.len();
            let sheet_name = node
                .data
                .get("sheetName")
                .and_then(|v| v.as_str())
                .unwrap_or("Sheet1");

            let result = plugins::excel_export_plugin(
                input.clone(),
                serde_json::json!({
                    "sheetName": sheet_name,
                    "fileName": format!("export_{}.xlsx", node.id),
                }),
            );

            match result {
                Ok(output) => {
                    log_manager.info(
                        project_id,
                        node_id,
                        &format!("[{}] Exported {} items to Excel", label_of(node), count),
                    );
                    (output, format!("Exported {} items to Excel", count))
                }
                Err(e) => {
                    log_manager.error(
                        project_id,
                        node_id,
                        &format!("[{}] Excel export failed: {}", label_of(node), e),
                    );
                    (vec![], format!("Excel export error: {}", e))
                }
            }
        }

        "csvExport" | "databaseExport" => {
            let count = input.len();
            log_manager.info(
                project_id,
                node_id,
                &format!("[{}] Exporting {} items", label_of(node), count),
            );
            (input.clone(), format!("Exported {} items", count))
        }

        _ => {
            log_manager.debug(
                project_id,
                node_id,
                &format!(
                    "[{}] Unknown type '{}', passing through {} items",
                    label_of(node),
                    node.node_type,
                    input_count
                ),
            );
            (
                input,
                format!("Passed through {} items (unknown type)", input_count),
            )
        }
    };

    let output_count = output.len();
    node_outputs
        .write()
        .unwrap()
        .insert(node_id.to_string(), output.clone());

    steps.push(ExecutionStep {
        node_id: node_id.to_string(),
        node_label: label_of(node).to_string(),
        node_type: node.node_type.clone(),
        input_count,
        output_count,
        detail,
        output: output.clone(),
    });
}

fn node_inputs_read(
    node_id: &str,
    edges: &[PipelineEdge],
    node_outputs: &Arc<RwLock<HashMap<String, Vec<serde_json::Value>>>>,
) -> Vec<serde_json::Value> {
    let map = node_outputs.read().unwrap();
    let mut combined = Vec::new();
    for edge in edges {
        if edge.target == node_id {
            if let Some(output) = map.get(&edge.source) {
                combined.extend(output.iter().cloned());
            }
        }
    }
    combined
}

// ── Repository-based Pipeline (New Architecture) ─────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryPipelineResult {
    pub success: bool,
    pub phase: String,
    pub ingested: i64,
    pub matched: i64,
    pub processed: i64,
    pub failed: i64,
    pub actions: Vec<crate::finish_actions::ActionResult>,
    pub error: Option<String>,
}

pub async fn execute_repository_pipeline(
    config: &PipelineConfig,
    db_path: &std::path::Path,
    log_manager: &Arc<LogManager>,
    project_id: &str,
    mut python_engine: Option<&mut PythonPluginEngine>,
    cancellation: Option<&Arc<AtomicBool>>,
) -> RepositoryPipelineResult {
    let _log_guard = crate::logs::LogContextGuard::new(log_manager.clone(), project_id);

    log_manager.info(
        project_id,
        "pipeline",
        &format!("Repository pipeline started: {} nodes", config.nodes.len()),
    );

    // Helper to check cancellation
    let is_cancelled = || -> bool {
        cancellation
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(false)
    };

    let repo = match RawItemRepository::open(db_path) {
        Ok(r) => {
            r.ensure_tables().ok();
            // Recover items stuck in 'processing' from a previous crash/restart
            if let Ok(n) = r.reset_stale_processing_items() {
                if n > 0 {
                    log_manager.info(
                        project_id,
                        "pipeline",
                        &format!("Recovered {} stale 'processing' items -> 'pending'", n),
                    );
                }
            }
            // If there are 'done' URL items but no 'pending' items, reset them so the
            // pipeline can re-fetch detail pages (e.g. after extract_rules were added).
            let pending_count = r.count_by_status("pending").unwrap_or(0);
            let done_url_count = r.count_done_url_items().unwrap_or(0);
            if pending_count == 0 && done_url_count > 0 {
                if let Ok(n) = r.reset_done_url_items_to_pending() {
                    if n > 0 {
                        log_manager.info(
                            project_id,
                            "pipeline",
                            &format!("Reset {} done URL items -> pending for re-processing", n),
                        );
                    }
                }
            }
            r
        }
        Err(e) => {
            return RepositoryPipelineResult {
                success: false,
                phase: "init".into(),
                ingested: 0,
                matched: 0,
                processed: 0,
                failed: 0,
                actions: vec![],
                error: Some(e),
            }
        }
    };

    // ── Phase 1a: Check for Crawled Items & Data Fetching ───────────
    log_manager.info(
        project_id,
        "pipeline",
        "Phase 1a: Checking for existing crawled items",
    );

    let mut total_ingested = 0i64;
    let mut fetched_sources: Vec<FetchedData> = Vec::new();
    // Listing URLs handed back by a plugin (e.g. Oreka preprocessor rewrite).
    let mut plugin_listing_urls: Vec<FetchedDataPlaceholder> = Vec::new();

    // First check if we already have crawled items
    let crawled_items = match repo.get_crawled_items() {
        Ok(items) => items,
        Err(e) => {
            return RepositoryPipelineResult {
                success: false,
                phase: "fetching".into(),
                ingested: 0,
                matched: 0,
                processed: 0,
                failed: 0,
                actions: vec![],
                error: Some(e),
            };
        }
    };
    if !crawled_items.is_empty() {
        log_manager.info(
            project_id,
            "fetching",
            &format!(
                "Found {} existing crawled items, using those instead of fetching",
                crawled_items.len()
            ),
        );
        for item in crawled_items {
            log_manager.info(
                project_id,
                "fetching",
                &format!(
                    "Found cached raw item id={} source_url={} item_type={}",
                    item.id, item.source_url, item.item_type
                ),
            );
        }
    } else {
        // No crawled items, proceed with fetching
        log_manager.info(
            project_id,
            "pipeline",
            "No existing crawled items found, starting data fetching",
        );
        for node in &config.nodes {
            if is_cancelled() {
                log_manager.info(
                    project_id,
                    "pipeline",
                    "Pipeline cancelled during data fetching",
                );
                return RepositoryPipelineResult {
                    success: false,
                    phase: "fetching".into(),
                    ingested: total_ingested,
                    matched: 0,
                    processed: 0,
                    failed: 0,
                    actions: vec![],
                    error: Some("Cancelled by user".into()),
                };
            }

            if !matches!(
                node.node_type.as_str(),
                "start" | "dataSource" | "rssSource"
            ) {
                continue;
            }
            let node_label = node
                .data
                .get("label")
                .and_then(|v| v.as_str())
                .unwrap_or(&node.id);
            let source_type = node
                .data
                .get("sourceType")
                .and_then(|v| v.as_str())
                .unwrap_or("url");
            let source_value = node
                .data
                .get("sourceValue")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let plugin_source_type = node
                .data
                .get("pluginSourceType")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            log_manager.info(
                project_id,
                "fetching",
                &format!(
                    "[node={}] Processing node type={}, sourceType={}, pluginSourceType={:?}",
                    node_label, node.node_type, source_type, plugin_source_type
                ),
            );

            // Check if a Python plugin should handle this data source
            if let Some(ref pst) = plugin_source_type {
                if let Some(engine) = python_engine.as_deref_mut() {
                    let plugin_id = pst.strip_prefix("py-").unwrap_or(pst);
                    let call_config = crate::pipeline_config::build_plugin_config(
                        &node.data,
                        &source_value,
                        project_id,
                    );

                    // Warn if shop_url is missing from pluginConfig and source_value is empty
                    let plugin_config_has_shop_url = node
                        .data
                        .get("pluginConfig")
                        .and_then(|c| c.get("shop_url"))
                        .is_some();
                    if !plugin_config_has_shop_url && source_value.is_empty() {
                        log_manager.warn(
                            project_id,
                            "fetching",
                            &format!(
                                "[node={}] Plugin '{}' requires shop_url but sourceValue is empty and pluginConfig.shop_url is not set",
                                node_label, plugin_id
                            ),
                        );
                    }

                    log_manager.info(
                        project_id,
                        "fetching",
                        &format!(
                            "[node={}] Calling plugin '{}' data source with config",
                            node_label, plugin_id
                        ),
                    );

                    match engine.call_data_source(plugin_id, call_config) {
                        Ok(plugin_items) => {
                            log_manager.info(
                                project_id,
                                "fetching",
                                &format!(
                                    "[node={}] Plugin returned {} items",
                                    node_label,
                                    plugin_items.len()
                                ),
                            );
                            if !plugin_items.is_empty() {
                                // Convert plugin items to NewRawItem and save to repo.
                                // The plugin decides the item_type:
                                //   - "raw"         → store page HTML; saved as a crawled raw
                                //                     source (status='crawled') for the preprocessor
                                //                     + fetch-data stages to consume.
                                //   - "listing_url" → a rewritten listing URL; handed to
                                //                     Phase 1b Stage A as a listing page.
                                //   - "url"/"product" → already-resolved product URLs.
                                let mut listing_from_plugin: Vec<
                                    crate::pipeline::FetchedDataPlaceholder,
                                > = Vec::new();
                                // Map item_hash -> raw_content so we can persist the
                                // product items' structured JSON in crawl_data.
                                let mut hash_to_content: std::collections::HashMap<String, String> =
                                    std::collections::HashMap::new();
                                let new_items: Vec<crate::repository::NewRawItem> = plugin_items
                                    .iter()
                                    .filter_map(|item| {
                                        let item_type = item
                                            .get("item_type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("product");
                                        let url =
                                            item.get("url").and_then(|v| v.as_str()).unwrap_or("");
                                        let raw_content = item
                                            .get("raw_content")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());
                                        let source = item
                                            .get("source_url")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or(&source_value)
                                            .to_string();

                                        if item_type == "raw" {
                                            // Persist the raw HTML as a crawled source so
                                            // Phase 1b can read it back via get_crawled_items().
                                            if let Some(content) = raw_content.clone() {
                                                let _ =
                                                    repo.save_raw_source(&source, "raw", &content);
                                            }
                                            return None;
                                        }

                                        if item_type == "listing_url" {
                                            // Hand the rewritten URL to Phase 1b Stage A.
                                            listing_from_plugin.push(
                                                crate::pipeline::FetchedDataPlaceholder {
                                                    source_url: source.clone(),
                                                    raw_data: String::new(),
                                                    input_type: "html".into(),
                                                    is_listing_url: true,
                                                },
                                            );
                                            return None;
                                        }

                                        // Default: product/url item.
                                        let raw_json =
                                            serde_json::to_string(item).unwrap_or_default();
                                        let content_to_save = raw_content.unwrap_or(raw_json);
                                        // Hash from source_url (stable per item) so re-crawls
                                        // deduplicate correctly instead of creating duplicates
                                        // when the plugin returns a slightly different `url`.
                                        let hash_input = if !source.is_empty() {
                                            source.as_str()
                                        } else if !url.is_empty() {
                                            url
                                        } else {
                                            &content_to_save
                                        };
                                        let item_hash =
                                            crate::pipeline_config::simple_hash(hash_input);
                                        hash_to_content.insert(item_hash.clone(), content_to_save);
                                        Some(crate::repository::NewRawItem {
                                            source_url: source,
                                            item_type: item_type.to_string(),
                                            item_hash,
                                        })
                                    })
                                    .collect();

                                if !new_items.is_empty() {
                                    match repo.save_items(&new_items) {
                                        Ok(r) => {
                                            total_ingested += r.inserted;
                                            log_manager.info(
                                                project_id,
                                                "fetching",
                                                &format!(
                                                    "[node={}] Plugin data source: {} inserted, {} dup",
                                                    node_label, r.inserted, r.duplicated
                                                ),
                                            );
                                            for new_item in &new_items {
                                                if let Ok(item_id) = repo
                                                    .get_raw_item_id_by_hash(&new_item.item_hash)
                                                {
                                                    if let Some(content) =
                                                        hash_to_content.get(&new_item.item_hash)
                                                    {
                                                        if repo
                                                            .get_crawl_data_content(item_id)
                                                            .is_none()
                                                        {
                                                            let _ = repo.save_crawl_data(
                                                                item_id, "raw", content,
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            log_manager.error(
                                                project_id,
                                                "fetching",
                                                &format!(
                                                    "[node={}] Failed to save plugin items: {}",
                                                    node_label, e
                                                ),
                                            );
                                        }
                                    }
                                }

                                // Stash listing URLs produced by the plugin so Phase 1b
                                // Stage A can fetch + rewrite them into listing pages.
                                if !listing_from_plugin.is_empty() {
                                    log_manager.info(
                                        project_id,
                                        "fetching",
                                        &format!(
                                            "[node={}] Plugin produced {} listing URL(s) for Stage A",
                                            node_label,
                                            listing_from_plugin.len()
                                        ),
                                    );
                                    plugin_listing_urls.extend(listing_from_plugin);
                                }
                            }
                        }
                        Err(e) => {
                            log_manager.error(
                                project_id,
                                "fetching",
                                &format!("[node={}] Plugin data source failed: {}", node_label, e),
                            );
                        }
                    }
                    // Skip the normal URL/source_type processing for plugin-based sources
                    continue;
                } else {
                    log_manager.warn(
                        project_id,
                        "fetching",
                        &format!(
                            "[node={}] Plugin data source '{}' requested but Python engine not available",
                            node_label, pst
                        ),
                    );
                }
            }

            match source_type {
                "url" | "api" => {
                    let url = source_value.to_string();
                    let profile = extract_client_profile(&node.data);
                    let wait_for_selector =
                        node.data.get("waitForSelector").and_then(|v| v.as_str());
                    let wait_for_content = node.data.get("waitForContent").and_then(|v| v.as_str());
                    let wait_timeout_ms = node.data.get("waitTimeoutMs").and_then(|v| v.as_u64());

                    // Check for pagination config
                    let pagination_config =
                        crate::pipeline_config::extract_pagination_config(&node.data);
                    let has_pagination = pagination_config.is_some();

                    if has_pagination {
                        log_manager.info(
                            project_id,
                            "fetching",
                            &format!("[node={}] Pagination enabled", node_label),
                        );
                    }

                    log_manager.info(
                        project_id,
                        "fetching",
                        &format!(
                            "[node={}] Fetching: {} (client: {})",
                            node_label, url, profile.client_type
                        ),
                    );

                    let fetch_start = std::time::Instant::now();

                    // Execute pagination if configured, otherwise single fetch
                    let htmls = if let Some(pag_config) = pagination_config {
                        match execute_pagination_in_pipeline(
                            &url,
                            &pag_config,
                            &profile,
                            project_id,
                            &log_manager,
                            node_label,
                        )
                        .await
                        {
                            Ok(htmls) => {
                                log_manager.info(
                                    project_id,
                                    "fetching",
                                    &format!(
                                        "[node={}] Pagination completed: {} pages fetched",
                                        node_label,
                                        htmls.len()
                                    ),
                                );
                                htmls
                            }
                            Err(e) => {
                                log_manager.error(
                                    project_id,
                                    "fetching",
                                    &format!(
                                        "Pagination failed: {}, falling back to single fetch",
                                        e
                                    ),
                                );
                                // Fallback to single fetch
                                let (result, _) = fetch_single_page(
                                    &url,
                                    &profile,
                                    wait_for_selector,
                                    wait_for_content,
                                    wait_timeout_ms,
                                    project_id,
                                    &log_manager,
                                    node_label,
                                )
                                .await;
                                vec![result.unwrap_or_default()]
                            }
                        }
                    } else {
                        let (result, _) = fetch_single_page(
                            &url,
                            &profile,
                            wait_for_selector,
                            wait_for_content,
                            wait_timeout_ms,
                            project_id,
                            &log_manager,
                            node_label,
                        )
                        .await;
                        vec![result.unwrap_or_default()]
                    };

                    let fetch_elapsed = fetch_start.elapsed();

                    log_manager.info(
                        project_id,
                        "fetching",
                        &format!(
                            "[node={}] Fetch completed in {:.1}s",
                            node_label,
                            fetch_elapsed.as_secs_f64()
                        ),
                    );

                    // Combine all HTMLs from pagination into single data for preprocessing
                    let combined_html = htmls.join("\n");

                    if !combined_html.is_empty() {
                        log_manager.info(
                            project_id,
                            "fetching",
                            &format!("Fetched {} ({} bytes)", source_value, combined_html.len()),
                        );
                        fetched_sources.push(FetchedData {
                            source_url: source_value.to_string(),
                            raw_data: combined_html,
                            input_type: "html".into(),
                            chrome_session: None,
                            from_plugin_listing: false,
                        });
                    }
                }
                "csv" | "json" | "xml" | "text" => {
                    if let Ok(content) = std::fs::read_to_string(source_value) {
                        fetched_sources.push(FetchedData {
                            source_url: source_value.to_string(),
                            raw_data: content,
                            input_type: source_type.to_string(),
                            chrome_session: None,
                            from_plugin_listing: false,
                        });
                    }
                }
                _ => {
                    log_manager.warn(
                        project_id,
                        "fetching",
                        &format!("Unknown source type: {}", source_type),
                    );
                }
            }
        } // end of for loop over nodes

        // Fold plugin-produced listing URLs (already rewritten, e.g. Oreka
        // storeId rewrite) into fetched_sources so Phase 1b Stage A
        // fetches their HTML directly (no second preprocessor rewrite).
        for pl in plugin_listing_urls.drain(..) {
            fetched_sources.push(FetchedData {
                source_url: pl.source_url,
                raw_data: String::new(),
                input_type: "html".into(),
                chrome_session: None,
                from_plugin_listing: true,
            });
        }
    } // end of else block (when no crawled items found)

    // ── Save raw HTML to DB for debug ────────────────────────
    for f in &fetched_sources {
        if !f.raw_data.is_empty() {
            log_manager.info(
                project_id,
                "fetching",
                &format!("Saving raw source HTML ({} bytes) to DB", f.raw_data.len()),
            );
            if let Err(e) = repo.save_raw_source(&f.source_url, "raw", &f.raw_data) {
                log_manager.warn(
                    project_id,
                    "fetching",
                    &format!("Failed to save raw source: {}", e),
                );
            }
        }
    }

    // Reload every crawled raw item from DB (content lives in `crawl_data`)
    // into fetched_sources so Phase 1b Stage A can preprocess them
    // (store-ID extraction / URL rewrite). This covers both the cached
    // path and items freshly saved by the data-source plugin above.
    if let Ok(crawled) = repo.get_crawled_items() {
        for item in crawled {
            let already = fetched_sources
                .iter()
                .any(|f| f.source_url == item.source_url && !f.raw_data.is_empty());
            if already {
                continue;
            }
            if let Some(html) = repo.get_crawl_data_content(item.id) {
                fetched_sources.push(FetchedData {
                    source_url: item.source_url,
                    raw_data: html,
                    input_type: "html".into(),
                    chrome_session: None,
                    from_plugin_listing: false,
                });
            }
        }
    }

    // ── Phase 1b: Data Preprocessing ────────────────────────
    // Preprocessor role:
    //   - Extract store IDs / rewrite source URLs (platform-specific, e.g., Oreka)
    //   - NOT responsible for extracting product URLs (that belongs to fetchData node)
    log_manager.info(
        project_id,
        "pipeline",
        "Phase 1b: Data Preprocessing (store-ID / URL rewrite)",
    );

    let preprocessor_nodes = crate::pipeline_config::extract_preprocessors(config);
    // fetchData node carries the URL patterns for extracting product URLs from listing pages
    let fetch_data_config = crate::pipeline_config::extract_fetch_data_config(config);

    macro_rules! close_chrome_sessions {
        ($sources:expr) => {
            for f in $sources {
                if let Some(ref s) = f.chrome_session {
                    log_manager.info(
                        project_id,
                        "preprocessing",
                        &format!("Closing Chrome session (pid={})", s.pid),
                    );
                    request_clients::close_chrome_session(s);
                }
            }
        };
    }

    // New two-stage approach:
    // Stage A: preprocessor resolves store IDs and rewrites listing URLs.
    //          It now produces a list of (listing_url, listing_html) pairs.
    // Stage B: fetchData node URL patterns are applied to each listing page to
    //          extract individual product URLs → saved as raw items.

    struct ListingPage {
        listing_url: String,
        listing_html: String,
    }
    let mut listing_pages: Vec<ListingPage> = Vec::new();

    for fetched in &fetched_sources {
        if is_cancelled() {
            log_manager.info(
                project_id,
                "pipeline",
                "Pipeline cancelled during preprocessing",
            );
            close_chrome_sessions!(&fetched_sources);
            return RepositoryPipelineResult {
                success: false,
                phase: "preprocessing".into(),
                ingested: total_ingested,
                matched: 0,
                processed: 0,
                failed: 0,
                actions: vec![],
                error: Some("Cancelled by user".into()),
            };
        }

        // Find matching preprocessor config for this source
        let preproc_config = preprocessor_nodes
            .iter()
            .find(|p| p.input_type == fetched.input_type)
            .cloned()
            .unwrap_or_else(|| PreprocessorConfig {
                input_type: fetched.input_type.clone(),
                item_selector: None,
                url_patterns: vec![],
                extract_rules: vec![],
                csv_delimiter: None,
                csv_has_header: None,
                json_item_path: None,
                client_type: None,
                client_timeout_secs: None,
                client_headless: None,
                wait_for_selector: None,
                wait_for_content: None,
                wait_timeout_ms: None,
                extract_store_id: None,
                platform: None,
            });

        // Already-rewritten listing URL produced by a Python preprocessor
        // (e.g. Oreka storeId rewrite). Skip the rewrite step and fetch
        // its HTML directly so Stage B can extract product URLs.
        if fetched.from_plugin_listing {
            log_manager.info(
                project_id,
                "preprocessing",
                &format!(
                    "Stage A (plugin listing): fetching rewritten URL {}",
                    fetched.source_url
                ),
            );
            let client_profile = extract_client_profile(&serde_json::json!({}));
            let (html, _) = fetch_single_page(
                &fetched.source_url,
                &client_profile,
                None,
                None,
                None,
                project_id,
                &log_manager,
                "pre-1",
            )
            .await;
            if let Some(html) = html {
                if !html.is_empty() {
                    listing_pages.push(ListingPage {
                        listing_url: fetched.source_url.clone(),
                        listing_html: html,
                    });
                } else {
                    log_manager.warn(
                        project_id,
                        "preprocessing",
                        &format!(
                            "Stage A (plugin listing): empty HTML for {}",
                            fetched.source_url
                        ),
                    );
                }
            } else {
                log_manager.warn(
                    project_id,
                    "preprocessing",
                    &format!(
                        "Stage A (plugin listing): empty HTML for {}",
                        fetched.source_url
                    ),
                );
            }
            continue;
        }

        let auto_extract_store_id = fetched.source_url.contains("oreka.vn/store/")
            || fetched.source_url.contains("oreka.vn/mua-ban?");
        let do_store_id = preproc_config
            .extract_store_id
            .unwrap_or(auto_extract_store_id);

        log_manager.info(
            project_id,
            "preprocessing",
            &format!(
                "Stage A: source={} input_type={} do_store_id={} platform={:?}",
                fetched.source_url, fetched.input_type, do_store_id, preproc_config.platform
            ),
        );

        if do_store_id {
            // Stage A: Platform-specific preprocessing (Oreka store-ID extraction + URL rewrite)
            // The preprocessor fetches the store page, extracts storeId, builds the listing URL.
            // Then we hand that listing URL + html to Stage B.
            let listing_url_result = if let Some(engine) = python_engine.as_deref_mut() {
                // Python plugin preprocessor: returns listing URL items
                let data_json = serde_json::json!({
                    "raw_data": fetched.raw_data,
                    "source_url": fetched.source_url,
                    "config": preproc_config,
                });
                match engine.call_preprocessor_hook("oreka-shop-crawler", data_json) {
                    Ok(items) if !items.is_empty() => {
                        // Plugin returned items. A "listing_url" item means the
                        // preprocessor resolved the store listing URL — we fetch its
                        // HTML and hand it to Stage B (product-URL extraction) instead
                        // of skipping. Other item types (url/product) are saved directly.
                        // Collect EVERY listing_url item returned by the preprocessor
                        // (a plugin may emit one per pagination page so that all
                        // listing pages get fetched and scraped, not just the first).
                        let listing_urls: Vec<String> = items
                            .iter()
                            .filter(|it| it.item_type == "listing_url")
                            .map(|it| it.source_url.clone())
                            .filter(|u| !u.is_empty())
                            .collect();

                        if !listing_urls.is_empty() {
                            let client_profile = extract_client_profile(&serde_json::json!({}));
                            for lurl in &listing_urls {
                                let (lhtml, _) = fetch_single_page(
                                    lurl,
                                    &client_profile,
                                    None,
                                    None,
                                    None,
                                    project_id,
                                    &log_manager,
                                    "pre-1",
                                )
                                .await;
                                if let Some(h) = lhtml {
                                    if !h.is_empty() {
                                        listing_pages.push(ListingPage {
                                            listing_url: lurl.clone(),
                                            listing_html: h,
                                        });
                                    } else {
                                        log_manager.warn(
                                            project_id,
                                            "preprocessing",
                                            &format!(
                                                "[Python preprocessor] empty listing HTML for {}",
                                                lurl
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                        // Save url/product items (skip the listing_url one, already
                        // handled above — Stage B will create the product URL items).
                        let save_items: Vec<crate::repository::NewRawItem> = items
                            .iter()
                            .filter(|it| it.item_type != "listing_url")
                            .cloned()
                            .collect();
                        if !save_items.is_empty() {
                            match repo.save_items(&save_items) {
                                Ok(r) => {
                                    total_ingested += r.inserted;
                                    log_manager.info(
                                        project_id,
                                        "preprocessing",
                                        &format!(
                                            "[Python preprocessor] saved {} new items ({} dup)",
                                            r.inserted, r.duplicated
                                        ),
                                    );
                                }
                                Err(e) => {
                                    log_manager.error(
                                        project_id,
                                        "preprocessing",
                                        &format!("Save failed: {}", e),
                                    );
                                }
                            }
                        }
                        continue;
                    }
                    Ok(_) => {
                        log_manager.warn(
                            project_id,
                            "preprocessing",
                            "Python preprocessor returned no items; falling back to built-in stage A",
                        );
                        // Fallback to built-in
                        resolve_listing_url_builtin(
                            &fetched.raw_data,
                            &fetched.source_url,
                            &preproc_config,
                            project_id,
                            log_manager,
                        )
                    }
                    Err(e) => {
                        log_manager.warn(
                            project_id,
                            "preprocessing",
                            &format!(
                                "Python preprocessor failed ({}); falling back to built-in",
                                e
                            ),
                        );
                        resolve_listing_url_builtin(
                            &fetched.raw_data,
                            &fetched.source_url,
                            &preproc_config,
                            project_id,
                            log_manager,
                        )
                    }
                }
            } else {
                resolve_listing_url_builtin(
                    &fetched.raw_data,
                    &fetched.source_url,
                    &preproc_config,
                    project_id,
                    log_manager,
                )
            };

            if let Some((listing_url, listing_html)) = listing_url_result {
                log_manager.info(
                    project_id,
                    "preprocessing",
                    &format!(
                        "Resolved listing URL: {} ({} bytes)",
                        listing_url,
                        listing_html.len()
                    ),
                );
                listing_pages.push(ListingPage {
                    listing_url,
                    listing_html,
                });
            } else {
                log_manager.warn(
                    project_id,
                    "preprocessing",
                    &format!(
                        "Could not resolve listing URL for source: {}",
                        fetched.source_url
                    ),
                );
            }
        } else {
            // No store-ID rewriting needed — use the fetched HTML directly as a listing page
            listing_pages.push(ListingPage {
                listing_url: fetched.source_url.clone(),
                listing_html: fetched.raw_data.clone(),
            });
        }
    }

    // Stage B: Apply fetchData node URL patterns to extract product URLs from listing pages
    log_manager.info(
        project_id,
        "pipeline",
        &format!(
            "Phase 1b Stage B: extracting product URLs from {} listing pages using fetchData patterns ({} patterns)",
            listing_pages.len(),
            fetch_data_config.url_patterns.len()
        ),
    );

    for listing in &listing_pages {
        if listing.listing_html.is_empty() {
            log_manager.warn(
                project_id,
                "preprocessing",
                &format!(
                    "Listing page {} has empty HTML, skipping URL extraction",
                    listing.listing_url
                ),
            );
            continue;
        }

        // Use fetchData URL patterns to extract product URLs
        let extraction_config = if fetch_data_config.url_patterns.is_empty() {
            // No fetchData patterns — fall back to preprocessor patterns if any
            preprocessor_nodes
                .iter()
                .find(|p| p.input_type == "html")
                .cloned()
                .unwrap_or_else(|| PreprocessorConfig {
                    input_type: "html".into(),
                    item_selector: None,
                    url_patterns: vec![],
                    extract_rules: vec![],
                    csv_delimiter: None,
                    csv_has_header: None,
                    json_item_path: None,
                    client_type: None,
                    client_timeout_secs: None,
                    client_headless: None,
                    wait_for_selector: None,
                    wait_for_content: None,
                    wait_timeout_ms: None,
                    extract_store_id: Some(false),
                    platform: None,
                })
        } else {
            fetch_data_config.clone()
        };

        let result = DataPreprocessor::process_internal_pub(
            &listing.listing_html,
            &listing.listing_url,
            &extraction_config,
        );

        log_manager.info(
            project_id,
            "preprocessing",
            &format!(
                "Stage B: listing_url={} → {} URLs extracted",
                listing.listing_url, result.extracted_count
            ),
        );

        match repo.save_items(&result.items) {
            Ok(r) => {
                total_ingested += r.inserted;
                log_manager.info(
                    project_id,
                    "preprocessing",
                    &format!(
                        "Saved from listing {}: {} new, {} dup",
                        listing.listing_url, r.inserted, r.duplicated
                    ),
                );
            }
            Err(e) => {
                log_manager.error(project_id, "preprocessing", &e);
            }
        }
    }

    // ── Phase 2: Worker Matching ─────────────────────────────
    log_manager.info(project_id, "pipeline", "Phase 2: Worker Matching");

    if is_cancelled() {
        log_manager.info(
            project_id,
            "pipeline",
            "Pipeline cancelled before worker matching",
        );
        return RepositoryPipelineResult {
            success: false,
            phase: "worker_matching".into(),
            ingested: total_ingested,
            matched: 0,
            processed: 0,
            failed: 0,
            actions: vec![],
            error: Some("Cancelled by user".into()),
        };
    }

    let workers = crate::worker_engine::extract_workers(config);
    let mut pending_items = match repo.get_pending_items(10000) {
        Ok(items) => items,
        Err(e) => {
            return RepositoryPipelineResult {
                success: false,
                phase: "worker_matching".into(),
                ingested: total_ingested,
                matched: 0,
                processed: 0,
                failed: 0,
                actions: vec![],
                error: Some(e),
            }
        }
    };

    let match_result = match WorkerEngine::match_items(&repo, &workers, &mut pending_items) {
        Ok(r) => r,
        Err(e) => {
            return RepositoryPipelineResult {
                success: false,
                phase: "worker_matching".into(),
                ingested: total_ingested,
                matched: 0,
                processed: 0,
                failed: 0,
                actions: vec![],
                error: Some(e),
            }
        }
    };

    log_manager.info(
        project_id,
        "worker_matching",
        &format!(
            "Matched: {}, unmatched: {}, ignored: {}",
            match_result.matched, match_result.unmatched, match_result.ignored
        ),
    );

    // ── Phase 3: Processing ──────────────────────────────────
    log_manager.info(
        project_id,
        "pipeline",
        "Phase 3: Worker Processing (chain of processors)",
    );

    let mut total_processed = 0i64;
    let mut total_failed = 0i64;

    let process_fn = |processor_type: &str,
                      _cfg: &serde_json::Value,
                      data: &serde_json::Value|
     -> Result<serde_json::Value, String> {
        let config_json = _cfg.clone();
        let result =
            plugins::execute_processor_static(processor_type, vec![data.clone()], config_json);
        if result.success {
            Ok(serde_json::Value::Array(result.data))
        } else {
            Err(result
                .error
                .unwrap_or_else(|| "Processor failed".to_string()))
        }
    };

    for worker in &workers {
        if is_cancelled() {
            log_manager.info(
                project_id,
                "pipeline",
                "Pipeline cancelled during processing",
            );
            return RepositoryPipelineResult {
                success: false,
                phase: "processing".into(),
                ingested: total_ingested,
                matched: match_result.matched,
                processed: total_processed,
                failed: total_failed,
                actions: vec![],
                error: Some("Cancelled by user".into()),
            };
        }

        let items = match repo.get_matched_items_for_worker(&worker.id, 1000) {
            Ok(items) => items,
            Err(e) => {
                log_manager.error(
                    project_id,
                    "processing",
                    &format!("Worker {}: {}", worker.id, e),
                );
                continue;
            }
        };

        if items.is_empty() {
            log_manager.info(
                project_id,
                "processing",
                &format!("Worker '{}': no matched items to process", worker.name),
            );
            continue;
        }

        log_manager.info(
            project_id,
            "processing",
            &format!(
                "Worker '{}': processing {} items (max_retries={})",
                worker.name,
                items.len(),
                worker.max_retries
            ),
        );

        // Chunk items for processing
        let chunk_size = worker.chunk_size.max(1);
        let chunks = WorkerEngine::chunk_items(items.clone(), chunk_size);
        log_manager.info(
            project_id,
            "processing",
            &format!(
                "Worker '{}': {} items split into {} chunk(s) of size {}",
                worker.name,
                items.len(),
                chunks.len(),
                chunk_size
            ),
        );

        for chunk in &chunks {
            match WorkerEngine::process_items_with_retry(
                &repo,
                worker,
                chunk,
                &process_fn,
                worker.max_retries,
            ) {
                Ok(result) => {
                    total_processed += result.processed;
                    total_failed += result.failed;
                    log_manager.info(
                        project_id,
                        "processing",
                        &format!(
                            "Worker '{}' chunk: {} processed, {} failed",
                            worker.name, result.processed, result.failed
                        ),
                    );
                }
                Err(e) => {
                    total_failed += chunk.len() as i64;
                    log_manager.error(
                        project_id,
                        "processing",
                        &format!("Worker '{}' chunk error: {}", worker.name, e),
                    );
                }
            }
        }
    }

    // ── Phase 4: Finish Actions ──────────────────────────────
    log_manager.info(project_id, "pipeline", "Phase 4: Finish Actions");

    if is_cancelled() {
        log_manager.info(
            project_id,
            "pipeline",
            "Pipeline cancelled before finish actions",
        );
        return RepositoryPipelineResult {
            success: false,
            phase: "finish_actions".into(),
            ingested: total_ingested,
            matched: match_result.matched,
            processed: total_processed,
            failed: total_failed,
            actions: vec![],
            error: Some("Cancelled by user".into()),
        };
    }

    let finish_actions = crate::finish_actions::extract_finish_actions(config, project_id);
    let log_fn = |msg: &str, level: &str| match level {
        "error" => {
            log_manager.error(project_id, "finish_actions", msg);
        }
        "warn" => {
            log_manager.warn(project_id, "finish_actions", msg);
        }
        _ => {
            log_manager.info(project_id, "finish_actions", msg);
        }
    };

    let action_results =
        match ActionEngine::execute_actions(&finish_actions, &repo, project_id, &log_fn) {
            Ok(results) => results,
            Err(e) => {
                log_manager.error(project_id, "finish_actions", &e);
                vec![]
            }
        };

    let success = total_failed == 0;
    log_manager.info(
        project_id,
        "pipeline",
        &format!(
            "Pipeline complete: ingested={}, matched={}, processed={}, failed={}",
            total_ingested, match_result.matched, total_processed, total_failed
        ),
    );

    RepositoryPipelineResult {
        success,
        phase: "done".into(),
        ingested: total_ingested,
        matched: match_result.matched,
        processed: total_processed,
        failed: total_failed,
        actions: action_results,
        error: None,
    }
}

/// Stage A helper: extract store ID from HTML and fetch the listing page.
/// Returns (listing_url, listing_html) or None if extraction failed.
fn resolve_listing_url_builtin(
    raw_html: &str,
    source_url: &str,
    config: &PreprocessorConfig,
    project_id: &str,
    log_manager: &Arc<LogManager>,
) -> Option<(String, String)> {
    use crate::data_preprocessor::DataPreprocessor;

    let store_id = DataPreprocessor::extract_store_id_pub(raw_html, source_url);
    if let Some(ref sid) = store_id {
        log_manager.info(
            project_id,
            "preprocessing",
            &format!(
                "[Stage A] Extracted storeId={} from source={}",
                sid, source_url
            ),
        );
    } else {
        log_manager.warn(
            project_id,
            "preprocessing",
            &format!(
                "[Stage A] Could not extract storeId from source={}",
                source_url
            ),
        );
        return None;
    }

    let store_id = store_id.unwrap();
    let listing_url = DataPreprocessor::build_listing_url_pub(source_url, &store_id);

    log_manager.info(
        project_id,
        "preprocessing",
        &format!("[Stage A] Fetching listing page: {}", listing_url),
    );

    let listing_html = DataPreprocessor::refetch_pub(&listing_url, config).unwrap_or_default();

    if listing_html.is_empty() {
        log_manager.warn(
            project_id,
            "preprocessing",
            &format!(
                "[Stage A] Listing page returned empty HTML for: {}",
                listing_url
            ),
        );
        return None;
    }

    Some((listing_url, listing_html))
}

// ── Pagination Helpers ─────────────────────────────────────

async fn fetch_single_page(
    url: &str,
    profile: &crate::models::ClientProfile,
    wait_for_selector: Option<&str>,
    wait_for_content: Option<&str>,
    wait_timeout_ms: Option<u64>,
    project_id: &str,
    log_manager: &crate::logs::LogManager,
    node_label: &str,
) -> (Option<String>, Option<crate::models::ChromeSession>) {
    let mut crawl_result = request_clients::fetch_with_client(
        url,
        profile,
        None,
        wait_for_selector,
        wait_for_content,
        wait_timeout_ms,
    )
    .await;

    // Fallback to HTTP client if chrome fails
    if crawl_result.error.is_some() && profile.client_type == "chrome" {
        log_manager.warn(
            project_id,
            "fetching",
            &format!(
                "[node={}] Chrome failed: {}, trying HTTP client",
                node_label,
                crawl_result.error.as_ref().unwrap()
            ),
        );
        let http_profile = crate::models::ClientProfile {
            client_type: "reqwest".to_string(),
            timeout_secs: profile.timeout_secs,
            user_agent: profile.user_agent.clone(),
            proxy_url: profile.proxy_url.clone(),
            headers: profile.headers.clone(),
            ..Default::default()
        };
        crawl_result =
            request_clients::fetch_with_client(url, &http_profile, None, None, None, None).await;
    }

    let html = if crawl_result.error.is_some() {
        log_manager.error(
            project_id,
            "fetching",
            &format!(
                "[node={}] Fetch failed: {}",
                node_label,
                crawl_result.error.as_ref().unwrap()
            ),
        );
        None
    } else {
        let html = crawl_result.html.unwrap_or_default();
        log_manager.info(
            project_id,
            "fetching",
            &format!(
                "[node={}] Fetched {} ({} bytes)",
                node_label,
                url,
                html.len()
            ),
        );
        // Log HTML snippet (first 500 chars)
        let html_snippet = html.chars().take(500).collect::<String>();
        log_manager.debug(
            project_id,
            "fetching",
            &format!("[node={}] HTML snippet: {}", node_label, html_snippet),
        );
        Some(html)
    };

    (html, None)
}

async fn execute_pagination_in_pipeline(
    base_url: &str,
    config: &crate::models::PaginationConfig,
    profile: &crate::models::ClientProfile,
    project_id: &str,
    log_manager: &crate::logs::LogManager,
    node_label: &str,
) -> Result<Vec<String>, String> {
    use crate::pagination::{execute_pagination, PaginationStrategy, UrlParameterPagination};

    let strategy: Box<dyn PaginationStrategy> = match config.pagination_type {
        crate::models::PaginationType::UrlParameter => {
            Box::new(UrlParameterPagination::new(config.clone()))
        }
        _ => {
            return Err("Pagination type not yet implemented".to_string());
        }
    };

    log_manager.info(
        project_id,
        "fetching",
        &format!(
            "[node={}] Starting pagination with type: {:?}",
            node_label, config.pagination_type
        ),
    );

    execute_pagination(base_url, config, profile, strategy.as_ref()).await
}

// ── Config Extractors ─────────────────────────────────────

/// FetchData node config — holds URL patterns for extracting product URLs from listing pages.
#[derive(Debug, Clone)]
pub struct FetchDataConfig {
    pub url_patterns: Vec<UrlPattern>,
    pub item_selector: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item_matcher::MatchPattern;

    fn make_node(id: &str, node_type: &str, data: serde_json::Value) -> PipelineNode {
        PipelineNode {
            id: id.to_string(),
            node_type: node_type.to_string(),
            label: None,
            data,
            position: None,
        }
    }

    fn make_edge(source: &str, target: &str) -> PipelineEdge {
        PipelineEdge {
            id: format!("{}-{}", source, target),
            source: source.to_string(),
            target: target.to_string(),
            source_handle: None,
            target_handle: None,
        }
    }

    #[test]
    fn test_extract_preprocessors_empty() {
        let config = PipelineConfig {
            nodes: vec![],
            edges: vec![],
            settings: serde_json::Value::Null,
        };
        let preprocs = crate::pipeline_config::extract_preprocessors(&config);
        assert!(preprocs.is_empty());
    }

    #[test]
    fn test_extract_preprocessors_with_html_node() {
        let config = PipelineConfig {
            nodes: vec![make_node(
                "pre-1",
                "preprocessor",
                serde_json::json!({
                    "inputType": "html",
                    "itemSelector": ".product-item",
                    "urlPatterns": [
                        {"enabled": true, "type": "contains", "value": "/product/"}
                    ]
                }),
            )],
            edges: vec![],
            settings: serde_json::Value::Null,
        };
        let preprocs = crate::pipeline_config::extract_preprocessors(&config);
        assert_eq!(preprocs.len(), 1);
        assert_eq!(preprocs[0].input_type, "html");
        assert_eq!(preprocs[0].item_selector.as_deref(), Some(".product-item"));
        assert_eq!(preprocs[0].url_patterns.len(), 1);
    }

    #[test]
    fn test_extract_preprocessors_filters_non_preprocessor() {
        let config = PipelineConfig {
            nodes: vec![
                make_node("ds-1", "start", serde_json::json!({"sourceType": "url"})),
                make_node(
                    "pre-1",
                    "preprocessor",
                    serde_json::json!({"inputType": "html"}),
                ),
                make_node(
                    "proc-1",
                    "processor",
                    serde_json::json!({"processorType": "rust-deduplicate"}),
                ),
            ],
            edges: vec![],
            settings: serde_json::Value::Null,
        };
        let preprocs = crate::pipeline_config::extract_preprocessors(&config);
        assert_eq!(preprocs.len(), 1);
    }

    #[test]
    fn test_extract_preprocessors_multiple_input_types() {
        let config = PipelineConfig {
            nodes: vec![
                make_node(
                    "pre-html",
                    "preprocessor",
                    serde_json::json!({
                        "inputType": "html",
                        "itemSelector": ".item",
                    }),
                ),
                make_node(
                    "pre-csv",
                    "preprocessor",
                    serde_json::json!({
                        "inputType": "csv",
                        "csvDelimiter": ";",
                        "csvHasHeader": true,
                    }),
                ),
                make_node(
                    "pre-json",
                    "preprocessor",
                    serde_json::json!({
                        "inputType": "json",
                        "jsonItemPath": "data.items",
                    }),
                ),
            ],
            edges: vec![],
            settings: serde_json::Value::Null,
        };
        let preprocs = crate::pipeline_config::extract_preprocessors(&config);
        assert_eq!(preprocs.len(), 3);

        let html = preprocs.iter().find(|p| p.input_type == "html").unwrap();
        assert_eq!(html.item_selector.as_deref(), Some(".item"));

        let csv = preprocs.iter().find(|p| p.input_type == "csv").unwrap();
        assert_eq!(csv.csv_delimiter.as_deref(), Some(";"));
        assert_eq!(csv.csv_has_header, Some(true));

        let json = preprocs.iter().find(|p| p.input_type == "json").unwrap();
        assert_eq!(json.json_item_path.as_deref(), Some("data.items"));
    }

    #[test]
    fn test_extract_preprocessors_with_extract_rules() {
        let config = PipelineConfig {
            nodes: vec![make_node(
                "pre-1",
                "preprocessor",
                serde_json::json!({
                    "inputType": "html",
                    "extractRules": [
                        {"type": "title", "value": ".product-title", "attribute": null},
                        {"type": "price", "value": ".price", "attribute": "data-value"},
                    ]
                }),
            )],
            edges: vec![],
            settings: serde_json::Value::Null,
        };
        let preprocs = crate::pipeline_config::extract_preprocessors(&config);
        assert_eq!(preprocs.len(), 1);
        assert_eq!(preprocs[0].extract_rules.len(), 2);
        assert_eq!(preprocs[0].extract_rules[0].rule_type, "title");
        assert_eq!(
            preprocs[0].extract_rules[1].attribute.as_deref(),
            Some("data-value")
        );
    }

    #[test]
    fn test_topological_sort_order() {
        let nodes = vec![
            make_node("a", "start", serde_json::Value::Null),
            make_node("b", "processor", serde_json::Value::Null),
            make_node("c", "processor", serde_json::Value::Null),
        ];
        let edges = vec![make_edge("a", "b"), make_edge("b", "c")];
        let sorted = topological_sort(&nodes, &edges).unwrap();
        let pos_a = sorted.iter().position(|x| x == "a").unwrap();
        let pos_b = sorted.iter().position(|x| x == "b").unwrap();
        let pos_c = sorted.iter().position(|x| x == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_extract_workers_with_url_format_type() {
        let config = PipelineConfig {
            nodes: vec![make_node(
                "w-1",
                "worker",
                serde_json::json!({
                    "detectionRules": [
                        {
                            "type": "url-format",
                            "value": ".*-detail\\/[0-9]{1,}\\/?"
                        }
                    ]
                }),
            )],
            edges: vec![],
            settings: serde_json::Value::Null,
        };
        let workers = crate::worker_engine::extract_workers(&config);
        assert_eq!(workers.len(), 1);
        assert_eq!(workers[0].matching_rules.len(), 1);
        assert_eq!(workers[0].matching_rules[0].field, "url");
        assert!(matches!(
            workers[0].matching_rules[0].pattern,
            MatchPattern::Regex(_)
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_pipeline_with_oreka_shop() {
        let config = PipelineConfig {
            nodes: vec![
                PipelineNode {
                    id: "ds-oreka".into(),
                    node_type: "start".into(),
                    label: Some("Oreka Shop Crawler".into()),
                    data: serde_json::json!({
                        "sourceType": "url",
                        "sourceValue": "https://www.oreka.vn/store/C21AVGZS44L3UU",
                        "pluginSourceType": "py-oreka-shop-crawler",
                        "pluginConfig": {},
                        "clientType": "static",
                        "clientHeadless": false,
                        "clientTimeoutSecs": 30,
                        "waitForSelector": "",
                        "waitTimeout": 30,
                    }),
                    position: None,
                },
                PipelineNode {
                    id: "1".into(),
                    node_type: "preprocessor".into(),
                    label: Some("Pre-processing".into()),
                    data: serde_json::json!({
                        "inputType": "html",
                        "urlPatterns": [
                            {"enabled": true, "type": "regex", "value": ".*-detail\\/[0-9]{1,}\\/?"}
                        ],
                        "extractRules": [],
                        "csvDelimiter": "",
                        "csvHasHeader": false,
                        "itemSelector": "",
                        "jsonItemPath": "",
                    }),
                    position: None,
                },
                PipelineNode {
                    id: "repository-node".into(),
                    node_type: "repository".into(),
                    label: Some("Repository".into()),
                    data: serde_json::json!({}),
                    position: None,
                },
                PipelineNode {
                    id: "worker-1".into(),
                    node_type: "worker".into(),
                    label: Some("Worker Node".into()),
                    data: serde_json::json!({
                        "clientType": "static",
                        "clientHeadless": false,
                        "clientTimeoutSecs": 30,
                        "concurrency": 3,
                        "detectionRules": [
                            {
                                "type": "url-format",
                                "value": ".*-detail\\/[0-9]{1,}\\/?"
                            }
                        ],
                        "extractionRules": [],
                        "inputType": "html",
                    }),
                    position: None,
                },
                PipelineNode {
                    id: "2".into(),
                    node_type: "html-data-extractor".into(),
                    label: Some("HTML Data Extracting".into()),
                    data: serde_json::json!({
                        "extractionRules": [
                            {"attribute": "content", "name": "product_name", "selector": "meta[property='og:title']", "type": "attribute"},
                            {"attribute": "content", "name": "images", "selector": "meta[property='og:image']", "type": "attribute"},
                            {"attribute": "content", "name": "description", "selector": "meta[property='og:description']", "type": "attribute"},
                            {"name": "price", "selector": "span.price", "type": "text"}
                        ],
                        "inputType": "html",
                    }),
                    position: None,
                },
                PipelineNode {
                    id: "exp-oreka".into(),
                    node_type: "processor".into(),
                    label: Some("Excel Export".into()),
                    data: serde_json::json!({
                        "processorType": "generate-excel-file",
                        "spreadsheetFormat": "xlsx",
                        "exportFileName": "san-pham-oreka",
                        "includeHeader": true,
                        "sheetName": "Oreka Products",
                        "columnMapping": {
                            "source_url": "URL",
                            "product_name": "Tên sản phẩm",
                            "price": "Giá",
                            "description": "Mô tả",
                            "images": "Hình ảnh"
                        }
                    }),
                    position: None,
                },
                PipelineNode {
                    id: "completion-node".into(),
                    node_type: "completion".into(),
                    label: Some("Completion".into()),
                    data: serde_json::json!({}),
                    position: None,
                },
            ],
            edges: vec![
                PipelineEdge {
                    id: "e1".into(),
                    source: "ds-oreka".into(),
                    target: "1".into(),
                    source_handle: None,
                    target_handle: None,
                },
                PipelineEdge {
                    id: "e2".into(),
                    source: "1".into(),
                    target: "repository-node".into(),
                    source_handle: None,
                    target_handle: None,
                },
                PipelineEdge {
                    id: "e3".into(),
                    source: "2".into(),
                    target: "worker-1".into(),
                    source_handle: None,
                    target_handle: None,
                },
                PipelineEdge {
                    id: "e4".into(),
                    source: "ds-oreka".into(),
                    target: "repository-node".into(),
                    source_handle: None,
                    target_handle: None,
                },
                PipelineEdge {
                    id: "e5".into(),
                    source: "worker-1".into(),
                    target: "exp-oreka".into(),
                    source_handle: None,
                    target_handle: None,
                },
                PipelineEdge {
                    id: "e6".into(),
                    source: "exp-oreka".into(),
                    target: "completion-node".into(),
                    source_handle: None,
                    target_handle: None,
                },
            ],
            settings: serde_json::json!({}),
        };

        let dir = std::env::temp_dir()
            .join("crawlflow-test")
            .join(&format!("pipeline_{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok();
        let db_path = dir.join("test.db");

        // Initialize Python plugin engine with the Oreka shop plugin
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let builtin_plugins_dir = manifest_dir.parent().map(|p| p.join("plugins"));

        let user_plugin_dir = std::env::temp_dir().join("crawlflow-test-plugins");
        std::fs::create_dir_all(&user_plugin_dir).ok();
        let mut py_engine = crate::python_plugins::PythonPluginEngine::new(
            builtin_plugins_dir.clone(),
            user_plugin_dir,
        );

        let discovered = py_engine.discover().unwrap_or_default();
        eprintln!("Python plugins discovered: {:?}", discovered);

        let enabled: std::collections::HashSet<String> = ["oreka-shop-crawler"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        py_engine.retain_plugins(&enabled);

        let repo = RawItemRepository::open(&db_path).unwrap();
        repo.ensure_tables().ok();

        let lm = crate::logs::LogManager::new();

        let project_id = "test-oreka-shop";

        let result = execute_repository_pipeline(
            &config,
            &db_path,
            &Arc::new(lm),
            project_id,
            Some(&mut py_engine),
            None,
        )
        .await;

        eprintln!(
            "Pipeline result: success={}, ingested={}, matched={}, processed={}, failed={}, error={:?}",
            result.success,
            result.ingested,
            result.matched,
            result.processed,
            result.failed,
            result.error
        );

        // Query items from DB to verify
        let items = repo
            .query_items(&crate::repository::ItemsQuery {
                status: None,
                worker_id: None,
                search: None,
                matched: None,
                limit: 1000,
                offset: 0,
                sort_by: None,
                sort_dir: None,
            })
            .map(|p| p.items)
            .unwrap_or_default();

        eprintln!("Total items in repo: {}", items.len());
        for item in &items {
            eprintln!(
                "  id={} type={} status={} matched={} source_url={:?}",
                item.id, item.item_type, item.status, item.matched, item.source_url,
            );
        }

        // Should have at least 1 item (raw source saved)
        assert!(items.len() >= 1, "Expected at least 1 item from pipeline");

        // Print summary
        let url_items: Vec<_> = items.iter().filter(|i| i.item_type == "url").collect();
        let raw_items: Vec<_> = items.iter().filter(|i| i.item_type == "raw").collect();
        let product_items: Vec<_> = items.iter().filter(|i| i.item_type == "product").collect();
        eprintln!(
            "Summary: {} raw, {} url, {} product items",
            raw_items.len(),
            url_items.len(),
            product_items.len()
        );

        // ── Verify end-to-end output per new data architecture ──
        // 1. Pipeline must have processed (fetched detail + ran processor chain) some items.
        eprintln!("Processed count = {}", result.processed);
        assert!(
            result.processed > 0,
            "Expected pipeline to process (fetch detail + export) at least 1 item"
        );

        // 2. json_ld auto-extracted from the store page (raw item #1).
        let lds = repo.get_json_ld(1).unwrap_or_default();
        eprintln!("json_ld rows for raw item #1 = {}", lds.len());
        assert!(
            !lds.is_empty(),
            "Expected JSON-LD auto-extracted from crawled HTML"
        );

        // 3. parsed_data final output exists for processed url items.
        let final_count: i64 = items
            .iter()
            .filter(|i| i.item_type == "url")
            .take(1)
            .filter_map(|i| repo.get_final_parsed(i.id).ok().flatten())
            .count() as i64;
        eprintln!(
            "final parsed_data present for a sample url item = {}",
            final_count
        );
        assert!(
            final_count >= 1,
            "Expected final parsed_data for processed item"
        );

        // 4. Excel export file should have been produced for the project.
        let exports_dir = dirs_next::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.CrawlFlow.desktop")
            .join("exports");
        eprintln!("Checking exports dir: {:?}", exports_dir);
        let excel_files: Vec<_> = std::fs::read_dir(&exports_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        name.contains("san-pham-oreka")
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        eprintln!("excel files found = {}", excel_files.len());
        for f in &excel_files {
            eprintln!("  -> {:?}", f.path());
        }
        assert!(
            !excel_files.is_empty(),
            "Expected an Excel export file to be produced"
        );

        // Cleanup
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_topological_levels() {
        let order = vec!["a", "b", "c", "d"];
        let edges = vec![
            make_edge("a", "b"),
            make_edge("a", "c"),
            make_edge("b", "d"),
            make_edge("c", "d"),
        ];
        let levels = topological_levels(
            &order.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            &edges,
        );
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["a"]);
        assert_eq!(levels[1].len(), 2);
        assert!(levels[1].contains(&"b".to_string()));
        assert!(levels[1].contains(&"c".to_string()));
        assert_eq!(levels[2], vec!["d"]);
    }

    #[test]
    fn test_build_plugin_config_injects_shop_url_from_source_value() {
        let node_data = serde_json::json!({
            "sourceType": "url",
            "sourceValue": "https://www.oreka.vn/store/C21AVGZS44L3UU",
            "pluginSourceType": "py-oreka-shop-crawler",
            "pluginConfig": {}
        });
        let source_value = "https://www.oreka.vn/store/C21AVGZS44L3UU";
        let project_id = "test-project";
        let config =
            crate::pipeline_config::build_plugin_config(&node_data, source_value, project_id);
        assert_eq!(
            config.get("shop_url").and_then(|v| v.as_str()),
            Some("https://www.oreka.vn/store/C21AVGZS44L3UU")
        );
        assert_eq!(
            config.get("source_url").and_then(|v| v.as_str()),
            Some("https://www.oreka.vn/store/C21AVGZS44L3UU")
        );
        assert_eq!(
            config.get("project_id").and_then(|v| v.as_str()),
            Some("test-project")
        );
    }

    #[test]
    fn test_build_plugin_config_preserves_existing_shop_url() {
        let node_data = serde_json::json!({
            "sourceType": "url",
            "sourceValue": "https://www.oreka.vn/store/C21AVGZS44L3UU",
            "pluginSourceType": "py-oreka-shop-crawler",
            "pluginConfig": {
                "shop_url": "https://www.oreka.vn/store/EXISTING_SHOP"
            }
        });
        let config = crate::pipeline_config::build_plugin_config(&node_data, "", "test-project");
        assert_eq!(
            config.get("shop_url").and_then(|v| v.as_str()),
            Some("https://www.oreka.vn/store/EXISTING_SHOP")
        );
    }

    #[test]
    fn test_build_plugin_config_does_not_inject_when_source_value_empty() {
        let node_data = serde_json::json!({
            "sourceType": "url",
            "sourceValue": "",
            "pluginSourceType": "py-oreka-shop-crawler",
            "pluginConfig": {}
        });
        let config = crate::pipeline_config::build_plugin_config(&node_data, "", "test-project");
        assert!(config.get("shop_url").is_none());
    }

    #[test]
    fn test_build_plugin_config_works_without_plugin_config() {
        let node_data = serde_json::json!({
            "sourceType": "url",
            "sourceValue": "https://example.com"
        });
        let config = crate::pipeline_config::build_plugin_config(
            &node_data,
            "https://example.com",
            "test-project",
        );
        assert_eq!(
            config.get("shop_url").and_then(|v| v.as_str()),
            Some("https://example.com")
        );
    }
}

fn label_of(node: &PipelineNode) -> &str {
    node.label.as_deref().unwrap_or(&node.node_type)
}
