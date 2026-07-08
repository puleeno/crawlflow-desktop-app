use crate::data_preprocessor::{DataPreprocessor, ExtractRule, PreprocessorConfig, UrlPattern};
use crate::finish_actions::{ActionEngine, FinishAction};
use crate::item_matcher::{MatchPattern, MatchRule};
use crate::logs::LogManager;
use crate::models::ClientProfile;
use crate::plugins;
use crate::python_plugins::PythonPluginEngine;
use crate::repository::RawItemRepository;
use crate::request_clients;
use crate::worker_engine::{ProcessorStep, WorkerDef, WorkerEngine};
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
                .unwrap_or(serde_json::Value::Null);

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

    // ── Phase 1a: Data Fetching ─────────────────────────────
    log_manager.info(project_id, "pipeline", "Phase 1a: Data Fetching");

    struct FetchedData {
        source_url: String,
        raw_data: String,
        input_type: String,
        chrome_session: Option<crate::models::ChromeSession>,
    }

    let mut total_ingested = 0i64;
    let mut fetched_sources: Vec<FetchedData> = Vec::new();
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

        log_manager.info(
            project_id,
            "fetching",
            &format!(
                "[node={}] Processing node type={}, sourceType={}",
                node_label, node.node_type, source_type
            ),
        );

        match source_type {
            "url" | "api" => {
                let url = source_value.to_string();
                let profile = extract_client_profile(&node.data);
                let wait_for_selector = node.data.get("waitForSelector").and_then(|v| v.as_str());
                let wait_for_content = node.data.get("waitForContent").and_then(|v| v.as_str());
                let wait_timeout_ms = node.data.get("waitTimeoutMs").and_then(|v| v.as_u64());

                // Check for pagination config
                let pagination_config = extract_pagination_config(&node.data);
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
                    ).await {
                        Ok(htmls) => {
                            log_manager.info(
                                project_id,
                                "fetching",
                                &format!("[node={}] Pagination completed: {} pages fetched", node_label, htmls.len()),
                            );
                            htmls
                        }
                        Err(e) => {
                            log_manager.error(project_id, "fetching", &format!("Pagination failed: {}, falling back to single fetch", e));
                            // Fallback to single fetch
                            let (result, _) = fetch_single_page(&url, &profile, wait_for_selector, wait_for_content, wait_timeout_ms, project_id, &log_manager, node_label).await;
                            vec![result.unwrap_or_default()]
                        }
                    }
                } else {
                    let (result, _) = fetch_single_page(&url, &profile, wait_for_selector, wait_for_content, wait_timeout_ms, project_id, &log_manager, node_label).await;
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
    }

    // ── Save raw HTML to DB for debug ────────────────────────
    for f in &fetched_sources {
        if !f.raw_data.is_empty() {
            log_manager.info(
                project_id,
                "fetching",
                &format!("Saving raw source HTML ({} bytes) to DB", f.raw_data.len()),
            );
            if let Err(e) = repo.save_raw_source(&f.source_url, &f.raw_data) {
                log_manager.warn(project_id, "fetching", &format!("Failed to save raw source: {}", e));
            }
        }
    }

    // ── Phase 1b: Data Preprocessing ────────────────────────
    log_manager.info(project_id, "pipeline", "Phase 1b: Data Preprocessing");

    let preprocessor_nodes = extract_preprocessors(config);

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

        // Tìm preprocessor node phù hợp cho source này (theo input_type)
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
            });

        let result = if let Some(engine) = python_engine.as_deref_mut() {
            DataPreprocessor::process_with_plugins(
                &fetched.raw_data,
                &fetched.source_url,
                &preproc_config,
                engine,
            )
        } else {
            // Use async version to support re-fetching with custom client settings
            DataPreprocessor::process_async(&fetched.raw_data, &fetched.source_url, &preproc_config)
                .await
        };

        match repo.save_items(&result.items) {
            Ok(r) => {
                total_ingested += r.inserted;
                log_manager.info(
                    project_id,
                    "preprocessing",
                    &format!(
                        "Source {}: {} extracted, {} new, {} dup",
                        fetched.source_url, result.extracted_count, r.inserted, r.duplicated
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

    let workers = extract_workers(config);
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
    log_manager.info(project_id, "pipeline", "Phase 3: Chain of Processors");

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

        let items = match repo.get_matched_items(&worker.id, 1000) {
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
            continue;
        }

        match WorkerEngine::process_items(&repo, worker, &items, &process_fn) {
            Ok(result) => {
                total_processed += result.processed;
                total_failed += result.failed;
                log_manager.info(
                    project_id,
                    "processing",
                    &format!(
                        "Worker '{}': {} processed, {} failed",
                        worker.name, result.processed, result.failed
                    ),
                );
            }
            Err(e) => {
                total_failed += items.len() as i64;
                log_manager.error(
                    project_id,
                    "processing",
                    &format!("Worker '{}' error: {}", worker.name, e),
                );
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

    let finish_actions = extract_finish_actions(config);
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

fn simple_hash(input: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

// ── Pagination Helpers ─────────────────────────────────────

fn extract_pagination_config(data: &serde_json::Value) -> Option<crate::models::PaginationConfig> {
    let pag_data = data.get("pagination")?;
    Some(serde_json::from_value(pag_data.clone()).ok()?)
}

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
    ).await;

    // Fallback to HTTP client if chrome fails
    if crawl_result.error.is_some() && profile.client_type == "chrome" {
        log_manager.warn(project_id, "fetching",
            &format!("[node={}] Chrome failed: {}, trying HTTP client", node_label, crawl_result.error.as_ref().unwrap()));
        let http_profile = crate::models::ClientProfile {
            client_type: "reqwest".to_string(),
            timeout_secs: profile.timeout_secs,
            user_agent: profile.user_agent.clone(),
            proxy_url: profile.proxy_url.clone(),
            headers: profile.headers.clone(),
            ..Default::default()
        };
        crawl_result = request_clients::fetch_with_client(
            url,
            &http_profile,
            None,
            None,
            None,
            None,
        ).await;
    }

    let html = if crawl_result.error.is_some() {
        log_manager.error(project_id, "fetching",
            &format!("[node={}] Fetch failed: {}", node_label, crawl_result.error.as_ref().unwrap()));
        None
    } else {
        let html = crawl_result.html.unwrap_or_default();
        log_manager.info(project_id, "fetching",
            &format!("[node={}] Fetched {} ({} bytes)", node_label, url, html.len()));
        // Log HTML snippet (first 500 chars)
        let html_snippet = html.chars().take(500).collect::<String>();
        log_manager.debug(project_id, "fetching",
            &format!("[node={}] HTML snippet: {}", node_label, html_snippet));
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
    use crate::pagination::{PaginationStrategy, UrlParameterPagination, execute_pagination};
    
    let strategy: Box<dyn PaginationStrategy> = match config.pagination_type {
        crate::models::PaginationType::UrlParameter => {
            Box::new(UrlParameterPagination::new(config.clone()))
        }
        _ => {
            return Err("Pagination type not yet implemented".to_string());
        }
    };

    log_manager.info(project_id, "fetching",
        &format!("[node={}] Starting pagination with type: {:?}", node_label, config.pagination_type));

    execute_pagination(base_url, config, profile, strategy.as_ref()).await
}

// ── Config Extractors ─────────────────────────────────────

/// Extract preprocessor configs từ các preprocessor nodes
fn extract_preprocessors(config: &PipelineConfig) -> Vec<PreprocessorConfig> {
    config
        .nodes
        .iter()
        .filter_map(|node| {
            if node.node_type != "preprocessor" {
                return None;
            }
            let data = &node.data;
            Some(PreprocessorConfig {
                input_type: data
                    .get("inputType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("html")
                    .to_string(),
                item_selector: data
                    .get("itemSelector")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                url_patterns: data
                    .get("urlPatterns")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|p| {
                                Some(UrlPattern {
                                    enabled: p
                                        .get("enabled")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(true),
                                    pattern_type: p
                                        .get("type")
                                        .and_then(|v| v.as_str())?
                                        .to_string(),
                                    value: p.get("value").and_then(|v| v.as_str())?.to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                extract_rules: data
                    .get("extractRules")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|r| {
                                Some(ExtractRule {
                                    rule_type: r.get("type").and_then(|v| v.as_str())?.to_string(),
                                    value: r.get("value").and_then(|v| v.as_str())?.to_string(),
                                    attribute: r
                                        .get("attribute")
                                        .and_then(|v| v.as_str())
                                        .map(String::from),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                csv_delimiter: data
                    .get("csvDelimiter")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                csv_has_header: data.get("csvHasHeader").and_then(|v| v.as_bool()),
                json_item_path: data
                    .get("jsonItemPath")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                client_type: data
                    .get("clientType")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                client_timeout_secs: data.get("clientTimeoutSecs").and_then(|v| v.as_u64()),
                client_headless: data.get("clientHeadless").and_then(|v| v.as_bool()),
                wait_for_selector: data
                    .get("waitForSelector")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                wait_for_content: data
                    .get("waitForContent")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                wait_timeout_ms: data.get("waitTimeoutMs").and_then(|v| v.as_u64()),
            })
        })
        .collect()
}

fn extract_workers(config: &PipelineConfig) -> Vec<WorkerDef> {
    let mut workers = Vec::new();

    for node in &config.nodes {
        if node.node_type != "worker" && node.node_type != "processor" {
            continue;
        }

        let matching_rules: Vec<MatchRule> = node
            .data
            .get("detectionRules")
            .or_else(|| node.data.get("matchingRules"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| {
                        let field = r.get("field").and_then(|v| v.as_str()).unwrap_or("url");
                        let pattern_type = r.get("type").and_then(|v| v.as_str())?;
                        let value = r.get("value").and_then(|v| v.as_str())?;
                        let negate = r.get("negate").and_then(|v| v.as_bool()).unwrap_or(false);
                        Some(MatchRule {
                            field: field.to_string(),
                            pattern: match pattern_type {
                                "wildcard" => MatchPattern::Wildcard(value.into()),
                                "regex" => MatchPattern::Regex(value.into()),
                                "contains" => MatchPattern::Contains(value.into()),
                                "startswith" => MatchPattern::StartsWith(value.into()),
                                "endswith" => MatchPattern::EndsWith(value.into()),
                                "always" => MatchPattern::Always,
                                _ => return None,
                            },
                            negate,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut processor_chain = Vec::new();
        let mut current_id = Some(node.id.as_str());
        let mut visited = std::collections::HashSet::new();

        while let Some(cid) = current_id.take() {
            if !visited.insert(cid.to_string()) {
                break;
            }

            if let Some(n) = config.nodes.iter().find(|n| n.id == cid) {
                if n.id != node.id {
                    processor_chain.push(ProcessorStep {
                        id: n.id.clone(),
                        processor_type: n
                            .data
                            .get("processorType")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&n.node_type)
                            .to_string(),
                        config: n
                            .data
                            .get("processorConfig")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null),
                    });
                }
            }

            if let Some(next_edge) = config.edges.iter().find(|e| e.source == cid) {
                current_id = Some(next_edge.target.as_str());
            }
        }

        workers.push(WorkerDef {
            id: node.id.clone(),
            name: node.label.clone().unwrap_or_else(|| node.id.clone()),
            matching_rules,
            processor_chain,
        });
    }

    workers
}

fn extract_finish_actions(config: &PipelineConfig) -> Vec<FinishAction> {
    let mut actions = Vec::new();

    for node in &config.nodes {
        let action = match node.node_type.as_str() {
            "excelExport" => Some(FinishAction::ExportExcel {
                path: node
                    .data
                    .get("outputPath")
                    .and_then(|v| v.as_str())
                    .unwrap_or("output.xlsx")
                    .to_string(),
                fields: node
                    .data
                    .get("fields")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
            }),
            "csvExport" => Some(FinishAction::ExportCsv {
                path: node
                    .data
                    .get("outputPath")
                    .and_then(|v| v.as_str())
                    .unwrap_or("output.csv")
                    .to_string(),
                fields: node
                    .data
                    .get("fields")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
            }),
            "completion" | "finish" => Some(FinishAction::LogSummary),
            _ => None,
        };

        if let Some(a) = action {
            actions.push(a);
        }
    }

    if actions.is_empty() {
        actions.push(FinishAction::LogSummary);
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let preprocs = extract_preprocessors(&config);
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
        let preprocs = extract_preprocessors(&config);
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
        let preprocs = extract_preprocessors(&config);
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
        let preprocs = extract_preprocessors(&config);
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
        let preprocs = extract_preprocessors(&config);
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
}

fn label_of(node: &PipelineNode) -> &str {
    node.label.as_deref().unwrap_or(&node.node_type)
}
