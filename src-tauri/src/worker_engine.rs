use crate::item_matcher::{ItemMatcher, MatchPattern, MatchRule};
use crate::models::ExtractRule as ModelsExtractRule;
use crate::pipeline::PipelineConfig;
use crate::pipeline_config::parse_extract_rules_array;
use crate::repository::{RawItem, RawItemRepository};
use serde::{Deserialize, Serialize};

// ── Worker Factory ─────────────────────────────────────

#[allow(dead_code)]
pub struct WorkerFactory;

#[allow(dead_code)]
impl WorkerFactory {
    /// Create worker definitions from pipeline node settings
    pub fn create_workers_from_nodes(nodes: &[crate::pipeline::PipelineNode]) -> Vec<WorkerDef> {
        let mut workers = Vec::new();

        for node in nodes {
            if node.node_type == "worker" {
                if let Some(worker_def) = Self::parse_worker_node(node) {
                    workers.push(worker_def);
                }
            }
        }

        workers
    }

    fn parse_worker_node(node: &crate::pipeline::PipelineNode) -> Option<WorkerDef> {
        let data = &node.data;

        // Parse detection rules into matching rules
        let detection_rules = data
            .get("detectionRules")
            .and_then(|v| v.as_array())
            .map(|rules| {
                rules
                    .iter()
                    .filter_map(|rule| {
                        let field = rule.get("field")?.as_str().unwrap_or("url");
                        let pattern_type = rule.get("type")?.as_str()?;
                        let value = rule.get("value")?.as_str()?;
                        let negate = rule.get("negate")?.as_bool().unwrap_or(false);

                        Some(crate::item_matcher::MatchRule {
                            field: field.to_string(),
                            pattern: match pattern_type {
                                "wildcard" => {
                                    crate::item_matcher::MatchPattern::Wildcard(value.to_string())
                                }
                                "regex" | "url-format" => {
                                    crate::item_matcher::MatchPattern::Regex(value.to_string())
                                }
                                "contains" => {
                                    crate::item_matcher::MatchPattern::Contains(value.to_string())
                                }
                                "startswith" => {
                                    crate::item_matcher::MatchPattern::StartsWith(value.to_string())
                                }
                                "endswith" => {
                                    crate::item_matcher::MatchPattern::EndsWith(value.to_string())
                                }
                                "always" => crate::item_matcher::MatchPattern::Always,
                                _ => crate::item_matcher::MatchPattern::Always,
                            },
                            negate,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Parse processor chain
        let processor_chain = data
            .get("processorChain")
            .and_then(|v| v.as_array())
            .map(|chain| {
                chain
                    .iter()
                    .filter_map(|step| {
                        let id = step.get("id")?.as_str().unwrap_or("").to_string();
                        let processor_type = step
                            .get("processorType")?
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        let config = step
                            .get("config")
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                        Some(ProcessorStep {
                            id,
                            processor_type,
                            config,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Parse client profile
        let client_profile = data
            .get("clientProfile")
            .map(|v| serde_json::from_value(v.clone()).unwrap_or_default());

        // Parse extract rules
        let extract_rules: Vec<crate::models::ExtractRule> = data
            .get("extractRules")
            .and_then(|v| v.as_array())
            .map(|rules| serde_json::from_value(serde_json::json!(rules)).unwrap_or_default())
            .unwrap_or_default();

        // Parse max_retries and chunk_size from node data
        let max_retries = data
            .get("maxRetries")
            .or_else(|| data.get("max_retries"))
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as u32;

        let chunk_size = data
            .get("chunkSize")
            .or_else(|| data.get("chunk_size"))
            .or_else(|| data.get("concurrency"))
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let column_mapping = data
            .get("settings")
            .and_then(|s| s.get("columnMapping"))
            .or_else(|| data.get("columnMapping"))
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        Some(WorkerDef {
            id: node.id.clone(),
            name: node.label.clone().unwrap_or_else(|| node.id.clone()),
            matching_rules: detection_rules,
            processor_chain,
            client_profile,
            extract_rules: if extract_rules.is_empty() {
                None
            } else {
                Some(extract_rules)
            },
            max_retries,
            chunk_size,
            column_mapping,
        })
    }
}

// ── Worker Definition ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerDef {
    pub id: String,
    pub name: String,
    pub matching_rules: Vec<MatchRule>,
    pub processor_chain: Vec<ProcessorStep>,
    /// HTTP client config for fetching detail pages (reqwest by default)
    pub client_profile: Option<crate::models::ClientProfile>,
    /// CSS-based extract rules to parse structured fields from the detail page HTML
    pub extract_rules: Option<Vec<crate::models::ExtractRule>>,
    /// Maximum number of retries per item before marking as ignored (default: 3)
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Number of items to process per chunk (default: 10)
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    /// Column mapping for Excel/CSV export: { field_name -> column_header }
    #[serde(default)]
    pub column_mapping: serde_json::Value,
}

fn default_max_retries() -> u32 {
    3
}
fn default_chunk_size() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorStep {
    pub id: String,
    pub processor_type: String,
    pub config: serde_json::Value,
}

// ── Worker Engine ─────────────────────────────────────────

pub struct WorkerEngine;

fn is_export_processor(kind: &str) -> bool {
    crate::plugins::is_export_processor(kind)
}

impl WorkerEngine {
    /// Chunk raw items into batches for parallel processing
    #[allow(dead_code)]
    pub fn chunk_items(items: Vec<RawItem>, chunk_size: usize) -> Vec<Vec<RawItem>> {
        items
            .chunks(chunk_size)
            .map(|chunk| chunk.to_vec())
            .collect()
    }

    /// Phase 2: Match pending items to workers.
    /// Items can match multiple workers (one-to-many relationship).
    /// Items that match no worker are marked 'ignored'.
    pub fn match_items(
        repo: &RawItemRepository,
        workers: &[WorkerDef],
        items: &mut Vec<RawItem>,
    ) -> Result<WorkerMatchResult, String> {
        let mut matched = 0i64;
        let mut unmatched = 0i64;
        let mut total_assignments = 0i64;

        for item in items.iter_mut() {
            let item_json = serde_json::json!({
                "source_url": item.source_url,
                "item_type": item.item_type,
            });

            let mut matched_workers = Vec::new();
            for worker in workers {
                let result = ItemMatcher::matches(&worker.matching_rules, &item_json);
                if result.matched {
                    matched_workers.push(worker.id.clone());
                }
            }

            if matched_workers.is_empty() {
                unmatched += 1;
            } else {
                // Assign all matching workers to this item
                for worker_id in &matched_workers {
                    repo.assign_worker(item.id, worker_id)?;
                    total_assignments += 1;
                }

                // Store first worker ID for backward compatibility
                item.worker_id = Some(matched_workers[0].clone());
                item.matched = 1;
                matched += 1;
            }
        }

        // Ignore unmatched items so they don't re-appear in future cycles
        let ignored = repo.ignore_unmatched()?;

        Ok(WorkerMatchResult {
            total: items.len() as i64,
            matched,
            unmatched,
            ignored,
            total_assignments,
        })
    }

    /// Phase 3: For each matched item:
    ///   1. Mark as "processing"
    ///   2. If item_type='url' → fetch the detail page via HTTP and parse with extract_rules
    ///   3. Run the processor chain on the structured data
    ///   4. Mark as "done" or "error" per item (no early-abort on single item failure)
    #[allow(dead_code)]
    pub fn process_items(
        repo: &RawItemRepository,
        worker: &WorkerDef,
        items: &[RawItem],
        execute_processor: &dyn Fn(
            &str,
            &serde_json::Value,
            &serde_json::Value,
        ) -> Result<serde_json::Value, String>,
        mut filter_parsed: Option<&mut dyn FnMut(&serde_json::Value) -> serde_json::Value>,
    ) -> Result<ProcessResult, String> {
        let mut processed = 0i64;
        let mut failed = 0i64;
        let mut results = Vec::new();

        for item in items {
            // Step 1 — mark as processing
            let _ = repo.update_status(item.id, "processing");

            // Step 2 — fetch detail page + parse HTML
            let parsed_data = match Self::fetch_and_parse_item(repo, item, worker) {
                Ok(data) => {
                    let filtered = if let Some(f) = filter_parsed.as_mut() {
                        f(&data)
                    } else {
                        data
                    };
                    log::info!(
                        "[worker::{}] Fetched+parsed item {} → {}",
                        worker.name,
                        item.id,
                        item.source_url
                    );
                    filtered
                }
                Err(e) => {
                    log::error!(
                        "[worker::{}] Fetch/parse failed for item {}: {}",
                        worker.name,
                        item.id,
                        e
                    );
                    let _ = repo.log_processing(
                        item.id,
                        Some(&worker.id),
                        "fetch_detail",
                        "error",
                        None,
                        Some(&e),
                    );
                    let _ = repo.update_status(item.id, "error");
                    failed += 1;
                    results.push(ProcessItemResult {
                        item_id: item.id,
                        source_url: item.source_url.clone(),
                        success: false,
                        steps: 0,
                        output: None,
                    });
                    continue;
                }
            };

            // Step 3 — run processor chain
            let mut current_data = parsed_data;
            let mut step_index = 0usize;
            let mut item_failed = false;

            for (step_idx, step) in worker.processor_chain.iter().enumerate() {
                // Export processors (excel/csv) are driven separately by the
                // service after parsing; they must NOT run inside the worker
                // chain, otherwise their result (a file path) becomes the
                // final_output instead of the extracted product fields.
                if is_export_processor(&step.processor_type) {
                    continue;
                }
                let _ = repo.log_processing(
                    item.id,
                    Some(&worker.id),
                    &step.processor_type,
                    "processing",
                    None,
                    None,
                );

                match execute_processor(&step.processor_type, &step.config, &current_data) {
                    Ok(output) => {
                        current_data = output;
                        let _ = repo.log_processing(
                            item.id,
                            Some(&worker.id),
                            &step.processor_type,
                            "done",
                            Some(&current_data.to_string()),
                            None,
                        );
                        step_index = step_idx;
                    }
                    Err(e) => {
                        let _ = repo.log_processing(
                            item.id,
                            Some(&worker.id),
                            &step.processor_type,
                            "error",
                            None,
                            Some(&e),
                        );
                        log::error!(
                            "[worker::{}] Processor '{}' failed on item {}: {}",
                            worker.name,
                            step.processor_type,
                            item.id,
                            e
                        );
                        let _ = repo.update_status(item.id, "error");
                        failed += 1;
                        item_failed = true;
                        results.push(ProcessItemResult {
                            item_id: item.id,
                            source_url: item.source_url.clone(),
                            success: false,
                            steps: step_index,
                            output: None,
                        });
                        break;
                    }
                }
            }

            if !item_failed {
                // Save the final extracted output to processing_log so that
                // get_done_items() can retrieve it for Excel/CSV export.
                // Without this, the export only sees raw repository fields
                // (id, source_url, status, text) instead of the configured
                // Data Extractor fields (title, price, images, etc.).
                let output_str = current_data.to_string();
                let _ = repo.log_processing(
                    item.id,
                    Some(&worker.id),
                    "final_output",
                    "done",
                    Some(&output_str),
                    None,
                );
                let _ = repo.update_status(item.id, "done");
                processed += 1;
                results.push(ProcessItemResult {
                    item_id: item.id,
                    source_url: item.source_url.clone(),
                    success: true,
                    steps: step_index + 1,
                    output: Some(current_data),
                });
            }
        }

        Ok(ProcessResult {
            total: items.len() as i64,
            processed,
            failed,
            results,
        })
    }

    /// Phase 3 with retry logic: Process items with automatic retry on failure.
    /// On max_retries exceeded, item is marked 'ignored' (not blocking the pipeline).
    pub fn process_items_with_retry(
        repo: &RawItemRepository,
        worker: &WorkerDef,
        items: &[RawItem],
        execute_processor: &dyn Fn(
            &str,
            &serde_json::Value,
            &serde_json::Value,
        ) -> Result<serde_json::Value, String>,
        mut filter_parsed: Option<&mut dyn FnMut(&serde_json::Value) -> serde_json::Value>,
        max_retries: u32,
        log_manager: Option<(&std::sync::Arc<crate::logs::LogManager>, &str)>,
    ) -> Result<ProcessResult, String> {
        let mut processed = 0i64;
        let mut failed = 0i64;
        let mut results = Vec::new();

        // Helper: emit to LogManager when available, fallback to Rust logger.
        let log_info = |msg: &str| {
            if let Some((lm, pid)) = log_manager {
                lm.info(pid, "processing", msg);
            } else {
                log::info!("{}", msg);
            }
        };
        let log_warn = |msg: &str| {
            if let Some((lm, pid)) = log_manager {
                lm.warn(pid, "processing", msg);
            } else {
                log::warn!("{}", msg);
            }
        };
        let log_error = |msg: &str| {
            if let Some((lm, pid)) = log_manager {
                lm.error(pid, "processing", msg);
            } else {
                log::error!("{}", msg);
            }
        };

        for item in items {
            let mut retry_count = 0;

            // Truncate long URLs for readable log lines.
            let short_url = if item.source_url.len() > 120 {
                format!("{}…", &item.source_url[..120])
            } else {
                item.source_url.clone()
            };

            loop {
                // Step 1 — mark as processing
                let _ = repo.update_status(item.id, "processing");

                if retry_count == 0 {
                    log_info(&format!(
                        "[worker={}] Fetching item #{} — {}",
                        worker.name, item.id, short_url
                    ));
                } else {
                    log_warn(&format!(
                        "[worker={}] Retry {}/{} for item #{} — {}",
                        worker.name, retry_count, max_retries, item.id, short_url
                    ));
                }

                // Step 2 — fetch detail page + parse HTML
                let fetch_start = std::time::Instant::now();
                let parsed_data = match Self::fetch_and_parse_item(repo, item, worker) {
                    Ok(data) => {
                        let elapsed_ms = fetch_start.elapsed().as_millis();
                        let filtered = if let Some(f) = filter_parsed.as_mut() {
                            f(&data)
                        } else {
                            data
                        };
                        // Count extracted fields for the log summary.
                        let field_count = filtered.as_object().map(|o| o.len()).unwrap_or(0);
                        log_info(&format!(
                            "[worker={}] Fetched+parsed item #{} in {}ms — {} fields extracted — {}",
                            worker.name, item.id, elapsed_ms, field_count, short_url
                        ));
                        filtered
                    }
                    Err(e) => {
                        let elapsed_ms = fetch_start.elapsed().as_millis();
                        log_error(&format!(
                            "[worker={}] Fetch failed item #{} in {}ms (attempt {}/{}) — {} — error: {}",
                            worker.name, item.id, elapsed_ms,
                            retry_count + 1, max_retries + 1,
                            short_url, e
                        ));
                        if retry_count < max_retries {
                            retry_count += 1;
                            std::thread::sleep(std::time::Duration::from_millis(
                                1000 * retry_count as u64,
                            ));
                            continue;
                        }

                        let _ = repo.log_processing(
                            item.id,
                            Some(&worker.id),
                            "fetch_detail",
                            "error",
                            None,
                            Some(&e),
                        );
                        let _ = repo.update_status(item.id, "error");
                        failed += 1;
                        results.push(ProcessItemResult {
                            item_id: item.id,
                            source_url: item.source_url.clone(),
                            success: false,
                            steps: 0,
                            output: None,
                        });
                        break;
                    }
                };

                // Step 3 — run processor chain
                let mut current_data = parsed_data;
                let mut step_index = 0usize;
                let mut item_failed = false;

                for (step_idx, step) in worker.processor_chain.iter().enumerate() {
                    if is_export_processor(&step.processor_type) {
                        continue;
                    }
                    let _ = repo.log_processing(
                        item.id,
                        Some(&worker.id),
                        &step.processor_type,
                        "processing",
                        None,
                        None,
                    );

                    log_info(&format!(
                        "[worker={}] item #{} — running processor '{}' (step {})",
                        worker.name, item.id, step.processor_type, step_idx + 1
                    ));

                    match execute_processor(&step.processor_type, &step.config, &current_data) {
                        Ok(output) => {
                            current_data = output;
                            let _ = repo.log_processing(
                                item.id,
                                Some(&worker.id),
                                &step.processor_type,
                                "done",
                                Some(&current_data.to_string()),
                                None,
                            );
                            step_index = step_idx;
                        }
                        Err(e) => {
                            if retry_count < max_retries {
                                retry_count += 1;
                                std::thread::sleep(std::time::Duration::from_millis(
                                    1000 * retry_count as u64,
                                ));
                                break; // Retry from fetch step
                            }

                            let _ = repo.log_processing(
                                item.id,
                                Some(&worker.id),
                                &step.processor_type,
                                "error",
                                None,
                                Some(&e),
                            );
                            log_error(&format!(
                                "[worker={}] Processor '{}' failed on item #{} (attempt {}/{}) — error: {}",
                                worker.name, step.processor_type, item.id,
                                retry_count + 1, max_retries + 1, e
                            ));
                            let _ = repo.update_status(item.id, "error");
                            failed += 1;
                            item_failed = true;
                            results.push(ProcessItemResult {
                                item_id: item.id,
                                source_url: item.source_url.clone(),
                                success: false,
                                steps: step_idx,
                                output: None,
                            });
                            break;
                        }
                    }
                }

                if !item_failed {
                    // Save final structured output to processing_log
                    let output_str = current_data.to_string();
                    let _ = repo.log_processing(
                        item.id,
                        Some(&worker.id),
                        "final_output",
                        "done",
                        Some(&output_str),
                        None,
                    );
                    let _ = repo.update_status(item.id, "done");
                    processed += 1;
                    log_info(&format!(
                        "[worker={}] Done item #{} — {} processor step(s) — {}",
                        worker.name, item.id, step_index + 1, short_url
                    ));
                    results.push(ProcessItemResult {
                        item_id: item.id,
                        source_url: item.source_url.clone(),
                        success: true,
                        steps: step_index + 1,
                        output: Some(current_data),
                    });
                    break; // Success, exit retry loop
                }
                // item_failed = true here means a processor step failed and retry loop
                // will restart from the top (fetch + process again)
            } // end retry loop
        } // end for item in items

        Ok(ProcessResult {
            total: items.len() as i64,
            processed,
            failed,
            results,
        })
    }

    // ── Detail Fetch & Parse ──────────────────────────────────

    /// Fetch the detail page for a matched item and parse it using the worker's
    /// extract_rules, returning a flat JSON object with all extracted fields.
    ///
    /// • item_type = 'url'  → makes an HTTP GET of source_url
    /// • item_type = 'url' → fetch detail page from network (Oreka flow:
    ///   worker fetches the product detail page).
    /// • item_type = 'page' / 'raw' / other → uses content from `crawl_data`
    ///   (previously fetched raw HTML), no network fetch.
    pub fn fetch_and_parse_item(
        repo: &RawItemRepository,
        item: &RawItem,
        worker: &WorkerDef,
    ) -> Result<serde_json::Value, String> {
        let url = &item.source_url;

        let html: String = if let Some(rc) = repo.get_crawl_data_content(item.id) {
            rc
        } else if item.item_type == "url" {
            let profile = worker.client_profile.clone().unwrap_or_default();
            Self::blocking_fetch(url, &profile)?
        } else {
            String::new()
        };

        // Build output map
        let mut fields: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        fields.insert("id".into(), serde_json::json!(item.id));
        fields.insert("url".into(), serde_json::json!(url));
        fields.insert("source_url".into(), serde_json::json!(item.source_url));
        fields.insert("item_type".into(), serde_json::json!(item.item_type));

        // Plugin-emitted structured item (item_type='product'): the plugin has
        // already parsed the data, so unwrap its JSON payload as the fields.
        // This lets Python plugins fully own the extraction logic.
        if item.item_type == "product" {
            if let Some(rc) = repo.get_crawl_data_content(item.id) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&rc) {
                    if let serde_json::Value::Object(obj) = parsed {
                        for (k, v) in obj {
                            fields.entry(k).or_insert(v);
                        }
                    }
                }
            }
            return Ok(serde_json::Value::Object(fields));
        }

        let extract_rules = worker.extract_rules.as_deref().unwrap_or(&[]);

        if !html.is_empty() {
            if extract_rules.is_empty() {
                // No rules → expose raw html and plain text
                fields.insert("html".into(), serde_json::json!(html));
                fields.insert(
                    "text".into(),
                    serde_json::json!(crate::crawler::strip_html_tags(&html)),
                );
            } else {
                // Apply CSS-based extraction
                let extracted = crate::crawler::extract_from_html(&html, extract_rules);
                for ef in extracted {
                    let val = if ef.values.len() == 1 {
                        serde_json::json!(ef.values[0])
                    } else {
                        serde_json::json!(ef.values)
                    };
                    fields.insert(ef.field, val);
                }
            }
        }

        Ok(serde_json::Value::Object(fields))
    }

    // ── HTTP Fetch (blocking, safe inside tokio) ──────────────

    /// Make a synchronous HTTP GET, safe to call from within a tokio async context.
    /// Uses `tokio::task::block_in_place` so the blocking reqwest runtime doesn't
    /// panic when dropped inside an existing tokio runtime.
    /// Delegates to `fetch_via_cdp` if client_type is "chrome" or "cdp".
    fn blocking_fetch(url: &str, profile: &crate::models::ClientProfile) -> Result<String, String> {
        if profile.client_type == "chrome" || profile.client_type == "cdp" {
            let (result, _) = crate::request_clients::fetch_via_cdp(
                url,
                profile,
                profile.wait_for_selector.as_deref(),
                None,
                None,
            );
            return result.html.ok_or_else(|| {
                result
                    .error
                    .unwrap_or_else(|| format!("Chrome/CDP fetch failed for {}", url))
            });
        }

        let url = url.to_string();
        let profile = profile.clone();

        // block_in_place: yield the tokio thread to blocking work without spawning
        // a new tokio runtime (which would panic when dropped inside an existing one).
        tokio::task::block_in_place(move || {
            // Load global network settings for keep-alive / timeout.
            let db_path = crate::commands::master_db_path();
            let net = crate::network_client::NetworkSettings::load_from_db(&db_path);
            let timeout = std::time::Duration::from_secs(
                profile.timeout_secs.unwrap_or(net.timeout_secs),
            );
            let mut builder = reqwest::blocking::Client::builder()
                .timeout(timeout)
                .danger_accept_invalid_certs(true)
                .connect_timeout(std::time::Duration::from_secs(10));

            if net.keep_alive {
                builder = builder
                    .tcp_keepalive(Some(std::time::Duration::from_secs(60)))
                    .tcp_nodelay(true);
            }

            if let Some(ref ua) = profile.user_agent {
                builder = builder.user_agent(ua.as_str());
            }

            let client = builder
                .build()
                .map_err(|e| format!("HTTP client build error: {}", e))?;

            let mut req = client.get(&url);
            if let Some(ref headers) = profile.headers {
                for (k, v) in headers {
                    if let (Ok(name), Ok(val)) = (
                        reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                        reqwest::header::HeaderValue::from_str(v),
                    ) {
                        req = req.header(name, val);
                    }
                }
            }

            let response = req
                .send()
                .map_err(|e| format!("HTTP request failed for {}: {}", url, e))?;

            let status = response.status().as_u16();
            if status < 200 || status >= 400 {
                return Err(format!("HTTP {} for {}", status, url));
            }

            response
                .text()
                .map_err(|e| format!("Failed to read response body: {}", e))
        })
    }
}

// ── Result Types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerMatchResult {
    pub total: i64,
    pub matched: i64,
    pub unmatched: i64,
    pub ignored: i64,
    pub total_assignments: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub total: i64,
    pub processed: i64,
    pub failed: i64,
    pub results: Vec<ProcessItemResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessItemResult {
    pub item_id: i64,
    pub source_url: String,
    pub success: bool,
    pub steps: usize,
    pub output: Option<serde_json::Value>,
}

// ── Worker extraction from pipeline graph ──────────────────────────

/// Build the list of [`WorkerDef`]s from the pipeline graph.
///
/// Only `worker` nodes become item-assignees. `processor` nodes are
/// downstream steps in a worker's chain (or finish actions) and must NOT be
/// standalone workers — otherwise they match all items (empty match rules)
/// and steal them from the real worker.
///
/// For each worker we walk its outgoing edges to collect the `processor_chain`,
/// and merge extract rules from any `html-data-extractor` node that feeds
/// into it (upstream) or sits downstream in its chain.
pub(crate) fn extract_workers(config: &PipelineConfig) -> Vec<WorkerDef> {
    let mut workers = Vec::new();

    for node in &config.nodes {
        if node.node_type != "worker" {
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
                                "regex" | "url-format" => MatchPattern::Regex(value.into()),
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
                            .or_else(|| n.data.get("settings").cloned())
                            .or_else(|| n.data.get("config").cloned())
                            .unwrap_or(serde_json::Value::Null),
                    });
                }
            }

            if let Some(next_edge) = config.edges.iter().find(|e| e.source == cid) {
                current_id = Some(next_edge.target.as_str());
            }
        }

        let client_type = node
            .data
            .get("clientType")
            .or_else(|| node.data.get("client_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("reqwest")
            .to_string();

        let timeout_secs = node
            .data
            .get("clientTimeoutSecs")
            .or_else(|| node.data.get("client_timeout_secs"))
            .or_else(|| node.data.get("timeout_secs"))
            .and_then(|v| v.as_u64())
            .or(Some(30));

        let headless = node
            .data
            .get("clientHeadless")
            .or_else(|| node.data.get("client_headless"))
            .or_else(|| node.data.get("headless"))
            .and_then(|v| v.as_bool())
            .or(Some(true));

        let wait_for_selector = node
            .data
            .get("waitForSelector")
            .or_else(|| node.data.get("wait_for_selector"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let client_profile = crate::models::ClientProfile {
            client_type,
            timeout_secs,
            headless,
            wait_for_selector,
            ..Default::default()
        };

        // --- Extract rules: first try the worker node's own data ---
        let mut extract_rules: Vec<ModelsExtractRule> = node
            .data
            .get("extractRules")
            .or_else(|| node.data.get("parserRules"))
            .or_else(|| node.data.get("rules"))
            .and_then(|v| v.as_array())
            .map(|arr| parse_extract_rules_array(arr))
            .unwrap_or_default();

        // --- Also pull rules from any html-data-extractor node connected to this
        //     worker (either as upstream feeder OR downstream consumer) ---
        let connected_extractor_ids: Vec<&str> = config
            .edges
            .iter()
            .filter(|e| e.target == node.id || e.source == node.id)
            .map(|e| {
                if e.source == node.id {
                    e.target.as_str()
                } else {
                    e.source.as_str()
                }
            })
            .collect();

        for extractor_id in connected_extractor_ids {
            if let Some(ext_node) = config.nodes.iter().find(|n| {
                n.id == extractor_id
                    && (n.node_type == "html-data-extractor"
                        || n.node_type == "htmlDataExtractor"
                        || n.node_type == "extractor")
            }) {
                let rules_from_extractor = ext_node
                    .data
                    .get("customRules")
                    .and_then(|v| v.as_array())
                    .or_else(|| {
                        ext_node
                            .data
                            .get("extractionRules")
                            .and_then(|v| v.as_array())
                    })
                    .or_else(|| ext_node.data.get("extractRules").and_then(|v| v.as_array()))
                    .or_else(|| ext_node.data.get("parserRules").and_then(|v| v.as_array()))
                    .or_else(|| ext_node.data.get("rules").and_then(|v| v.as_array()))
                    .map(|arr| parse_extract_rules_array(arr))
                    .unwrap_or_default();

                if !rules_from_extractor.is_empty() {
                    log::info!(
                        "[extract_workers] Merged {} extract rules from upstream html-data-extractor '{}' into worker '{}'",
                        rules_from_extractor.len(),
                        ext_node.id,
                        node.id
                    );
                    extract_rules.extend(rules_from_extractor);
                }
            }
        }

        // --- Also pull rules from DOWNSTREAM extractor nodes (in the processor_chain) ---
        for step in &processor_chain {
            if let Some(ext_node) = config.nodes.iter().find(|n| {
                n.id == step.id
                    && (n.node_type == "html-data-extractor"
                        || n.node_type == "htmlDataExtractor"
                        || n.node_type == "extractor")
            }) {
                let rules_from_downstream = ext_node
                    .data
                    .get("customRules")
                    .or_else(|| ext_node.data.get("extractionRules"))
                    .or_else(|| ext_node.data.get("extractRules"))
                    .or_else(|| ext_node.data.get("parserRules"))
                    .or_else(|| ext_node.data.get("rules"))
                    .and_then(|v| v.as_array())
                    .map(|arr| parse_extract_rules_array(arr))
                    .unwrap_or_default();

                if !rules_from_downstream.is_empty() {
                    log::info!(
                        "[extract_workers] Merged {} extract rules from downstream html-data-extractor '{}' into worker '{}'",
                        rules_from_downstream.len(),
                        ext_node.id,
                        node.id
                    );
                    extract_rules.extend(rules_from_downstream);
                }
            }
        }

        let max_retries = node
            .data
            .get("maxRetries")
            .or_else(|| node.data.get("max_retries"))
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as u32;

        let chunk_size = node
            .data
            .get("chunkSize")
            .or_else(|| node.data.get("chunk_size"))
            .or_else(|| node.data.get("concurrency"))
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        // Collect column mapping for Excel/CSV export from processor chain nodes
        let mut column_mapping: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for step in &processor_chain {
            if let Some(proc_node) = config.nodes.iter().find(|n| n.id == step.id) {
                let mapping = proc_node
                    .data
                    .get("settings")
                    .and_then(|s| s.get("columnMapping"))
                    .or_else(|| proc_node.data.get("columnMapping"))
                    .and_then(|v| v.as_object())
                    .cloned();
                if let Some(m) = mapping {
                    column_mapping.extend(m);
                }
            }
        }
        let worker_mapping = node
            .data
            .get("settings")
            .and_then(|s| s.get("columnMapping"))
            .or_else(|| node.data.get("columnMapping"))
            .and_then(|v| v.as_object())
            .cloned();
        if let Some(m) = worker_mapping {
            for (k, v) in m {
                column_mapping.entry(k).or_insert(v);
            }
        }

        // Build the list of field names produced by this worker's Data Extractor
        // settings (custom rules + preset rules merged from downstream extractor
        // nodes). Excel/CSV export must only emit these fields, not DB metadata.
        let extract_field_names: Vec<String> = extract_rules
            .iter()
            .map(|r| r.field.clone())
            .collect::<Vec<String>>();

        // Inject `extractFields` into the config of every Excel/CSV export step
        // so the export plugin knows which fields belong to the extractor output.
        let mut processor_chain = processor_chain;
        for step in processor_chain.iter_mut() {
            if crate::plugins::is_export_processor(&step.processor_type) {
                let mut cfg = if step.config.is_object() {
                    step.config.as_object().cloned().unwrap()
                } else {
                    serde_json::Map::new()
                };
                cfg.insert(
                    "extractFields".into(),
                    serde_json::json!(extract_field_names),
                );
                step.config = serde_json::Value::Object(cfg);
            }
        }

        workers.push(WorkerDef {
            id: node.id.clone(),
            name: node.label.clone().unwrap_or_else(|| node.id.clone()),
            matching_rules,
            processor_chain,
            client_profile: Some(client_profile),
            extract_rules: Some(extract_rules),
            max_retries,
            chunk_size,
            column_mapping: serde_json::Value::Object(column_mapping),
        });
    }

    workers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{PipelineConfig, PipelineEdge, PipelineNode};

    #[test]
    fn test_extract_workers_merges_extraction_rules_from_upstream_extractor() {
        // Extractor node uses the `extractionRules` key (Oreka preset style)
        // and feeds INTO the worker (upstream). The worker itself has an
        // empty `extractionRules`. The merged rules must surface on the
        // worker so export output contains the extractor fields, not DB metadata.
        let nodes: Vec<PipelineNode> = vec![
            serde_json::json!({
                "id": "ext-1",
                "type": "html-data-extractor",
                "data": {
                    "extractionRules": [
                        {"name": "product_name", "selector": "h1", "type": "text"},
                        {"name": "price", "selector": ".price", "type": "text"}
                    ]
                }
            }),
            serde_json::json!({
                "id": "worker-1",
                "type": "worker",
                "data": { "extractionRules": [] }
            }),
            serde_json::json!({
                "id": "proc-1",
                "type": "processor",
                "data": {"processorType": "generate-excel-file", "settings": {}}
            }),
        ]
        .into_iter()
        .map(|v| serde_json::from_value(v).unwrap())
        .collect();
        let edges: Vec<PipelineEdge> = vec![
            serde_json::json!({"id": "e1", "source": "ext-1", "target": "worker-1"}),
            serde_json::json!({"id": "e2", "source": "worker-1", "target": "proc-1"}),
        ]
        .into_iter()
        .map(|v| serde_json::from_value(v).unwrap())
        .collect();

        let config = PipelineConfig {
            nodes,
            edges,
            settings: serde_json::Value::Null,
        };

        let workers = extract_workers(&config);
        assert_eq!(workers.len(), 1);
        let w = &workers[0];
        let fields: Vec<String> = w
            .extract_rules
            .as_ref()
            .unwrap()
            .iter()
            .map(|r| r.field.clone())
            .collect();
        assert!(
            fields.contains(&"product_name".to_string()),
            "missing product_name: {:?}",
            fields
        );
        assert!(
            fields.contains(&"price".to_string()),
            "missing price: {:?}",
            fields
        );

        // The excel export step must receive `extractFields` listing those fields.
        assert_eq!(w.processor_chain.len(), 1);
        let cfg = &w.processor_chain[0].config;
        let ef = cfg.get("extractFields").and_then(|v| v.as_array()).unwrap();
        let ef_names: Vec<String> = ef.iter().map(|v| v.as_str().unwrap().to_string()).collect();
        assert!(ef_names.contains(&"product_name".to_string()));
        assert!(ef_names.contains(&"price".to_string()));
    }
}
