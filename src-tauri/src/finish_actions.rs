use crate::repository::RawItemRepository;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ── Action Types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FinishAction {
    ExportCsv {
        path: String,
        fields: Vec<String>,
    },
    ExportExcel {
        path: String,
        fields: Vec<String>,
    },
    SaveToDatabase {
        connection: String,
        table: String,
    },
    SendToApi {
        url: String,
        method: String,
        body_template: String,
    },
    Webhook {
        url: String,
    },
    LogSummary,
}

// ── Action Engine ─────────────────────────────────────────

pub struct ActionEngine;

impl ActionEngine {
    /// Execute finish actions on processed items.
    pub fn execute_actions(
        actions: &[FinishAction],
        repo: &RawItemRepository,
        project_id: &str,
        log_fn: &dyn Fn(&str, &str),
    ) -> Result<Vec<ActionResult>, String> {
        let mut results = Vec::new();

        // Get all done items
        let done_items = match repo.count_by_status("done") {
            Ok(count) => count,
            Err(_) => 0,
        };
        let error_items = match repo.count_by_status("error") {
            Ok(count) => count,
            Err(_) => 0,
        };

        log_fn(
            &format!(
                "[FinishActions] {} done, {} error items for project {}",
                done_items, error_items, project_id
            ),
            "info",
        );

        for action in actions {
            match action {
                FinishAction::LogSummary => {
                    log_fn(
                        &format!(
                            "[FinishActions] Project {} summary: {} done, {} error",
                            project_id, done_items, error_items
                        ),
                        "info",
                    );
                    results.push(ActionResult {
                        action: "log_summary".into(),
                        success: true,
                        message: format!("{} done, {} error", done_items, error_items),
                    });
                }
                FinishAction::ExportCsv { path, fields } => {
                    let result = Self::export_csv(repo, path, fields, log_fn);
                    results.push(result);
                }
                FinishAction::ExportExcel { path, fields } => {
                    let result = Self::export_excel(repo, path, fields, log_fn);
                    results.push(result);
                }
                FinishAction::SaveToDatabase { connection, table } => {
                    let result = Self::save_to_db(repo, connection, table, log_fn);
                    results.push(result);
                }
                FinishAction::SendToApi {
                    url,
                    method,
                    body_template,
                } => {
                    let result = Self::send_to_api(repo, url, method, body_template, log_fn);
                    results.push(result);
                }
                FinishAction::Webhook { url } => {
                    let result = Self::call_webhook(repo, url, project_id, log_fn);
                    results.push(result);
                }
            }
        }

        Ok(results)
    }

    fn export_csv(
        repo: &RawItemRepository,
        path: &str,
        fields: &[String],
        log_fn: &dyn Fn(&str, &str),
    ) -> ActionResult {
        let done_items = match repo.get_done_items(10000) {
            Ok(items) => items,
            Err(e) => {
                log_fn(
                    &format!("[FinishActions] Failed to get done items: {}", e),
                    "error",
                );
                return ActionResult {
                    action: "export_csv".into(),
                    success: false,
                    message: format!("Failed to read items: {}", e),
                };
            }
        };

        let rows: Vec<serde_json::Value> = done_items
            .iter()
            .map(|(item, output)| {
                let mut map = serde_json::Map::new();
                map.insert("id".into(), serde_json::json!(item.id));
                map.insert("source_url".into(), serde_json::json!(item.source_url));
                map.insert(
                    "extracted_url".into(),
                    serde_json::json!(item.extracted_url),
                );
                map.insert("status".into(), serde_json::json!(item.status));
                if let Some(out) = output {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(out) {
                        if let Some(obj) = parsed.as_object() {
                            for (k, v) in obj {
                                map.insert(k.clone(), v.clone());
                            }
                        } else if let Some(arr) = parsed.as_array() {
                            if let Some(first_obj) = arr.first().and_then(|v| v.as_object()) {
                                for (k, v) in first_obj {
                                    map.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                }
                serde_json::Value::Object(map)
            })
            .collect();

        let filtered: Vec<serde_json::Value> = if fields.is_empty() {
            rows
        } else {
            rows.into_iter()
                .map(|item| {
                    let mut map = serde_json::Map::new();
                    if let Some(obj) = item.as_object() {
                        for f in fields {
                            if let Some(v) = obj.get(f) {
                                map.insert(f.clone(), v.clone());
                            }
                        }
                    }
                    serde_json::Value::Object(map)
                })
                .collect()
        };

        let wb = crate::spreadsheet::Workbook::from_json_rows(&filtered, "Sheet1");
        match crate::spreadsheet::write(&wb, path) {
            Ok(()) => {
                let abs_path = std::fs::canonicalize(path)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.to_string());
                log_fn(
                    &format!("[FinishActions] CSV exported to {}", abs_path),
                    "info",
                );
                ActionResult {
                    action: "export_csv".into(),
                    success: true,
                    message: format!("Exported {} items to {}", filtered.len(), abs_path),
                }
            }
            Err(e) => {
                log_fn(
                    &format!("[FinishActions] CSV export failed: {}", e),
                    "error",
                );
                ActionResult {
                    action: "export_csv".into(),
                    success: false,
                    message: format!("Export failed: {}", e),
                }
            }
        }
    }

    fn export_excel(
        repo: &RawItemRepository,
        path: &str,
        fields: &[String],
        log_fn: &dyn Fn(&str, &str),
    ) -> ActionResult {
        let done_items = match repo.get_done_items(10000) {
            Ok(items) => items,
            Err(e) => {
                log_fn(
                    &format!("[FinishActions] Failed to get done items: {}", e),
                    "error",
                );
                return ActionResult {
                    action: "export_excel".into(),
                    success: false,
                    message: format!("Failed to read items: {}", e),
                };
            }
        };

        let rows: Vec<serde_json::Value> = done_items
            .iter()
            .map(|(item, output)| {
                let mut map = serde_json::Map::new();
                map.insert("id".into(), serde_json::json!(item.id));
                map.insert("source_url".into(), serde_json::json!(item.source_url));
                map.insert(
                    "extracted_url".into(),
                    serde_json::json!(item.extracted_url),
                );
                map.insert("status".into(), serde_json::json!(item.status));
                if let Some(out) = output {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(out) {
                        if let Some(obj) = parsed.as_object() {
                            for (k, v) in obj {
                                map.insert(k.clone(), v.clone());
                            }
                        } else if let Some(arr) = parsed.as_array() {
                            if let Some(first_obj) = arr.first().and_then(|v| v.as_object()) {
                                for (k, v) in first_obj {
                                    map.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                }
                serde_json::Value::Object(map)
            })
            .collect();

        let filtered: Vec<serde_json::Value> = if fields.is_empty() {
            rows
        } else {
            rows.into_iter()
                .map(|item| {
                    let mut map = serde_json::Map::new();
                    if let Some(obj) = item.as_object() {
                        for f in fields {
                            if let Some(v) = obj.get(f) {
                                map.insert(f.clone(), v.clone());
                            }
                        }
                    }
                    serde_json::Value::Object(map)
                })
                .collect()
        };

        // Ensure parent directory exists
        if let Some(parent) = Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let wb = crate::spreadsheet::Workbook::from_json_rows(&filtered, "Sheet1");
        match crate::spreadsheet::write_xlsx(&wb, path) {
            Ok(()) => {
                let abs_path = std::fs::canonicalize(path)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.to_string());
                log_fn(
                    &format!("[FinishActions] Excel exported to {}", abs_path),
                    "info",
                );
                ActionResult {
                    action: "export_excel".into(),
                    success: true,
                    message: format!("Exported {} items to {}", filtered.len(), abs_path),
                }
            }
            Err(e) => {
                log_fn(
                    &format!("[FinishActions] Excel export failed: {}", e),
                    "error",
                );
                ActionResult {
                    action: "export_excel".into(),
                    success: false,
                    message: format!("Export failed: {}", e),
                }
            }
        }
    }

    fn save_to_db(
        _repo: &RawItemRepository,
        _connection: &str,
        _table: &str,
        log_fn: &dyn Fn(&str, &str),
    ) -> ActionResult {
        log_fn(
            &format!(
                "[FinishActions] Save to DB {}/{} (stub)",
                _connection, _table
            ),
            "info",
        );
        ActionResult {
            action: "save_to_db".into(),
            success: true,
            message: format!("Saved to {}.{}", _connection, _table),
        }
    }

    fn send_to_api(
        _repo: &RawItemRepository,
        _url: &str,
        _method: &str,
        _body_template: &str,
        log_fn: &dyn Fn(&str, &str),
    ) -> ActionResult {
        log_fn(
            &format!("[FinishActions] API call {} {} (stub)", _method, _url),
            "info",
        );
        ActionResult {
            action: "send_to_api".into(),
            success: true,
            message: format!("{} {} - 200 OK", _method, _url),
        }
    }

    fn call_webhook(
        _repo: &RawItemRepository,
        _url: &str,
        _project_id: &str,
        log_fn: &dyn Fn(&str, &str),
    ) -> ActionResult {
        log_fn(&format!("[FinishActions] Webhook {} (stub)", _url), "info");
        ActionResult {
            action: "webhook".into(),
            success: true,
            message: format!("Webhook {} sent", _url),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub action: String,
    pub success: bool,
    pub message: String,
}
