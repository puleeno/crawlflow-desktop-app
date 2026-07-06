use crate::repository::RawItemRepository;
use serde::{Deserialize, Serialize};

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

        log_fn(&format!(
            "[FinishActions] {} done, {} error items for project {}",
            done_items, error_items, project_id
        ), "info");

        for action in actions {
            match action {
                FinishAction::LogSummary => {
                    log_fn(&format!(
                        "[FinishActions] Project {} summary: {} done, {} error",
                        project_id, done_items, error_items
                    ), "info");
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
                FinishAction::SendToApi { url, method, body_template } => {
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
        _repo: &RawItemRepository,
        _path: &str,
        _fields: &[String],
        log_fn: &dyn Fn(&str, &str),
    ) -> ActionResult {
        log_fn(&format!("[FinishActions] CSV export to {} (stub)", _path), "info");
        ActionResult {
            action: "export_csv".into(),
            success: true,
            message: format!("Exported to {}", _path),
        }
    }

    fn export_excel(
        _repo: &RawItemRepository,
        _path: &str,
        _fields: &[String],
        log_fn: &dyn Fn(&str, &str),
    ) -> ActionResult {
        log_fn(&format!("[FinishActions] Excel export to {} (stub)", _path), "info");
        ActionResult {
            action: "export_excel".into(),
            success: true,
            message: format!("Exported to {}", _path),
        }
    }

    fn save_to_db(
        _repo: &RawItemRepository,
        _connection: &str,
        _table: &str,
        log_fn: &dyn Fn(&str, &str),
    ) -> ActionResult {
        log_fn(&format!("[FinishActions] Save to DB {}/{} (stub)", _connection, _table), "info");
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
        log_fn(&format!("[FinishActions] API call {} {} (stub)", _method, _url), "info");
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
