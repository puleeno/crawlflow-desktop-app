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
    /// Phase 3: Match pending items to workers.
    /// Items that match a worker are assigned to it.
    /// Items that don't match any worker are ignored.
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

        // Ignore unmatched items
        let ignored = repo.ignore_unmatched()?;

        Ok(WorkerMatchResult {
            total: items.len() as i64,
            matched,
            unmatched,
            ignored,
        })
    }

    /// Phase 4: Execute processor chain on matched items for a worker.
    pub fn process_items(
        repo: &RawItemRepository,
        worker: &WorkerDef,
        items: &[RawItem],
        execute_processor: &dyn Fn(&str, &serde_json::Value, &serde_json::Value)
            -> Result<serde_json::Value, String>,
    ) -> Result<ProcessResult, String> {
        let mut processed = 0i64;
        let mut failed = 0i64;
        let mut results = Vec::new();

        for item in items {
            let item_data = serde_json::json!({
                "id": item.id,
                "source_url": item.source_url,
                "extracted_url": item.extracted_url,
                "raw_content": item.raw_content,
                "item_type": item.item_type,
            });

            let mut current_data = item_data.clone();
            let mut step_index = 0usize;

            for (step_idx, step) in worker.processor_chain.iter().enumerate() {
                // Log processing start
                repo.log_processing(
                    item.id, Some(&worker.id),
                    &step.processor_type, "processing",
                    None, None,
                )?;

                match execute_processor(&step.processor_type, &step.config, &current_data) {
                    Ok(output) => {
                        current_data = output;
                        repo.log_processing(
                            item.id, Some(&worker.id),
                            &step.processor_type, "done",
                            Some(&current_data.to_string()),
                            None,
                        )?;
                        step_index = step_idx;
                    }
                    Err(e) => {
                        repo.log_processing(
                            item.id, Some(&worker.id),
                            &step.processor_type, "error",
                            None, Some(&e),
                        )?;
                        repo.update_status(item.id, "error")?;
                        failed += 1;
                        return Err(format!("Processor '{}' failed on item {}: {}",
                            step.processor_type, item.id, e));
                    }
                }
            }

            repo.update_status(item.id, "done")?;
            processed += 1;
            results.push(ProcessItemResult {
                item_id: item.id,
                source_url: item.source_url.clone(),
                success: true,
                steps: step_index + 1,
                output: Some(current_data),
            });
        }

        Ok(ProcessResult {
            total: items.len() as i64,
            processed,
            failed,
            results,
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
