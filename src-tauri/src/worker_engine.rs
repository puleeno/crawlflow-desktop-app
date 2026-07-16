use crate::item_matcher::{ItemMatcher, MatchRule};
use crate::repository::{RawItem, RawItemRepository};
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessorStep {
    pub id: String,
    pub processor_type: String,
    pub config: serde_json::Value,
}

// ── Worker Engine ─────────────────────────────────────────

pub struct WorkerEngine;

impl WorkerEngine {
    /// Phase 2: Match pending items to workers.
    /// Items that match a worker are assigned to it.
    /// Items that match no worker are marked 'ignored'.
    pub fn match_items(
        repo: &RawItemRepository,
        workers: &[WorkerDef],
        items: &mut Vec<RawItem>,
    ) -> Result<WorkerMatchResult, String> {
        let mut matched = 0i64;
        let mut unmatched = 0i64;

        for item in items.iter_mut() {
            let item_json = serde_json::json!({
                "source_url": item.source_url,
                "extracted_url": item.extracted_url,
                "raw_content": item.raw_content,
                "item_type": item.item_type,
            });

            let mut item_matched = false;
            for worker in workers {
                let result = ItemMatcher::matches(&worker.matching_rules, &item_json);
                if result.matched {
                    repo.assign_worker(item.id, &worker.id)?;
                    item.worker_id = Some(worker.id.clone());
                    item.matched = 1;
                    item_matched = true;
                    matched += 1;
                    break;
                }
            }

            if !item_matched {
                unmatched += 1;
            }
        }

        // Ignore unmatched items so they don't re-appear in future cycles
        let ignored = repo.ignore_unmatched()?;

        Ok(WorkerMatchResult {
            total: items.len() as i64,
            matched,
            unmatched,
            ignored,
        })
    }

    /// Phase 3: For each matched item:
    ///   1. Mark as "processing"
    ///   2. If item_type='url' → fetch the detail page via HTTP and parse with extract_rules
    ///   3. Run the processor chain on the structured data
    ///   4. Mark as "done" or "error" per item (no early-abort on single item failure)
    pub fn process_items(
        repo: &RawItemRepository,
        worker: &WorkerDef,
        items: &[RawItem],
        execute_processor: &dyn Fn(
            &str,
            &serde_json::Value,
            &serde_json::Value,
        ) -> Result<serde_json::Value, String>,
    ) -> Result<ProcessResult, String> {
        let mut processed = 0i64;
        let mut failed = 0i64;
        let mut results = Vec::new();

        for item in items {
            // Step 1 — mark as processing
            let _ = repo.update_status(item.id, "processing");

            // Step 2 — fetch detail page + parse HTML
            let parsed_data = match Self::fetch_and_parse_item(item, worker) {
                Ok(data) => {
                    log::info!(
                        "[worker::{}] Fetched+parsed item {} → {}",
                        worker.name,
                        item.id,
                        item.extracted_url.as_deref().unwrap_or(&item.source_url)
                    );
                    data
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

    // ── Detail Fetch & Parse ──────────────────────────────────

    /// Fetch the detail page for a matched item and parse it using the worker's
    /// extract_rules, returning a flat JSON object with all extracted fields.
    ///
    /// • item_type = 'url'  → makes an HTTP GET of extracted_url (or source_url)
    /// • item_type = 'page' / other → skips fetch, uses existing raw_content
    pub fn fetch_and_parse_item(
        item: &RawItem,
        worker: &WorkerDef,
    ) -> Result<serde_json::Value, String> {
        let url = item
            .extracted_url
            .as_deref()
            .filter(|u| !u.is_empty())
            .unwrap_or(&item.source_url);

        let html: String = if item.item_type == "url" {
            let profile = worker.client_profile.clone().unwrap_or_default();
            Self::blocking_fetch(url, &profile)?
        } else {
            item.raw_content.clone().unwrap_or_default()
        };

        // Build output map
        let mut fields: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        fields.insert("id".into(), serde_json::json!(item.id));
        fields.insert("url".into(), serde_json::json!(url));
        fields.insert("source_url".into(), serde_json::json!(item.source_url));
        fields.insert(
            "extracted_url".into(),
            serde_json::json!(item.extracted_url),
        );
        fields.insert("item_type".into(), serde_json::json!(item.item_type));

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
            let timeout = std::time::Duration::from_secs(profile.timeout_secs.unwrap_or(30));
            let mut builder = reqwest::blocking::Client::builder()
                .timeout(timeout)
                .danger_accept_invalid_certs(true);

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
