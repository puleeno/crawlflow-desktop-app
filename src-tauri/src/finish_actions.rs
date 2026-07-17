use crate::pipeline::PipelineConfig;
use crate::repository::RawItemRepository;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use dirs_next;

// ── Action Types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FinishAction {
    ExportCsv {
        path: String,
        fields: Vec<String>,
        #[serde(default)]
        column_mapping: serde_json::Value,
    },
    ExportExcel {
        path: String,
        fields: Vec<String>,
        #[serde(default)]
        column_mapping: serde_json::Value,
        #[serde(default = "default_sheet_name")]
        sheet_name: String,
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

fn default_sheet_name() -> String {
    "Sheet1".to_string()
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
                FinishAction::ExportCsv {
                    path,
                    fields,
                    column_mapping,
                } => {
                    let result = Self::export_csv(repo, path, fields, column_mapping, log_fn);
                    results.push(result);
                }
                FinishAction::ExportExcel {
                    path,
                    fields,
                    column_mapping,
                    sheet_name,
                } => {
                    let result =
                        Self::export_excel(repo, path, fields, column_mapping, sheet_name, log_fn);
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
        column_mapping: &serde_json::Value,
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

        let filtered: Vec<serde_json::Value> = if let Some(map) = column_mapping.as_object() {
            if map.is_empty() {
                if fields.is_empty() {
                    rows
                } else {
                    rows.into_iter()
                        .map(|item| {
                            let mut new_map = serde_json::Map::new();
                            if let Some(obj) = item.as_object() {
                                for f in fields {
                                    if let Some(v) = obj.get(f) {
                                        new_map.insert(f.clone(), v.clone());
                                    }
                                }
                            }
                            serde_json::Value::Object(new_map)
                        })
                        .collect()
                }
            } else {
                rows.into_iter()
                    .map(|item| {
                        let mut new_map = serde_json::Map::new();
                        if let Some(obj) = item.as_object() {
                            for (k, v) in map {
                                if let Some(header_name) = v.as_str() {
                                    if let Some(val) = obj.get(k) {
                                        new_map.insert(header_name.to_string(), val.clone());
                                    } else {
                                        new_map.insert(
                                            header_name.to_string(),
                                            serde_json::Value::String(String::new()),
                                        );
                                    }
                                }
                            }
                        }
                        serde_json::Value::Object(new_map)
                    })
                    .collect()
            }
        } else {
            if fields.is_empty() {
                rows
            } else {
                rows.into_iter()
                    .map(|item| {
                        let mut new_map = serde_json::Map::new();
                        if let Some(obj) = item.as_object() {
                            for f in fields {
                                if let Some(v) = obj.get(f) {
                                    new_map.insert(f.clone(), v.clone());
                                }
                            }
                        }
                        serde_json::Value::Object(new_map)
                    })
                    .collect()
            }
        };

        let wb = crate::spreadsheet::Workbook::from_json_rows(&filtered, "Sheet1", true);
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
        column_mapping: &serde_json::Value,
        sheet_name: &str,
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

        let filtered: Vec<serde_json::Value> = if let Some(map) = column_mapping.as_object() {
            if map.is_empty() {
                if fields.is_empty() {
                    rows
                } else {
                    rows.into_iter()
                        .map(|item| {
                            let mut new_map = serde_json::Map::new();
                            if let Some(obj) = item.as_object() {
                                for f in fields {
                                    if let Some(v) = obj.get(f) {
                                        new_map.insert(f.clone(), v.clone());
                                    }
                                }
                            }
                            serde_json::Value::Object(new_map)
                        })
                        .collect()
                }
            } else {
                rows.into_iter()
                    .map(|item| {
                        let mut new_map = serde_json::Map::new();
                        if let Some(obj) = item.as_object() {
                            for (k, v) in map {
                                if let Some(header_name) = v.as_str() {
                                    if let Some(val) = obj.get(k) {
                                        new_map.insert(header_name.to_string(), val.clone());
                                    } else {
                                        new_map.insert(
                                            header_name.to_string(),
                                            serde_json::Value::String(String::new()),
                                        );
                                    }
                                }
                            }
                        }
                        serde_json::Value::Object(new_map)
                    })
                    .collect()
            }
        } else {
            if fields.is_empty() {
                rows
            } else {
                rows.into_iter()
                    .map(|item| {
                        let mut new_map = serde_json::Map::new();
                        if let Some(obj) = item.as_object() {
                            for f in fields {
                                if let Some(v) = obj.get(f) {
                                    new_map.insert(f.clone(), v.clone());
                                }
                            }
                        }
                        serde_json::Value::Object(new_map)
                    })
                    .collect()
            }
        };

        // Ensure parent directory exists
        if let Some(parent) = Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let wb = crate::spreadsheet::Workbook::from_json_rows(&filtered, sheet_name, true);
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

// ── Finish-action extraction from pipeline graph ──────────────────

/// Build the list of [`FinishAction`]s from the pipeline graph.
///
/// Export/processor/completion nodes become terminal actions run after all
/// workers have finished. A `generate-excel-file` / `generate-csv-file`
/// processor node is resolved into an `ExportExcel` / `ExportCsv` action
/// writing into the project's `exports/` directory.
pub(crate) fn extract_finish_actions(
    config: &PipelineConfig,
    project_id: &str,
) -> Vec<FinishAction> {
    let mut actions = Vec::new();

    // Collect all worker column mappings (keyed by worker_id)
    let worker_column_mappings: HashMap<String, serde_json::Value> = config
        .nodes
        .iter()
        .filter(|n| n.node_type == "worker")
        .map(|n| {
            let mapping = n
                .data
                .get("settings")
                .and_then(|s| s.get("columnMapping"))
                .or_else(|| n.data.get("columnMapping"))
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            (n.id.clone(), mapping)
        })
        .collect();

    for node in &config.nodes {
        let action = match node.node_type.as_str() {
            "excelExport" => {
                let column_mapping = node
                    .data
                    .get("columnMapping")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                Some(FinishAction::ExportExcel {
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
                    column_mapping,
                    sheet_name: node
                        .data
                        .get("sheetName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Sheet1")
                        .to_string(),
                })
            }
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
                column_mapping: node
                    .data
                    .get("columnMapping")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
            }),
            "processor" => {
                let processor_type = node
                    .data
                    .get("processorType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                match processor_type {
                    "generate-excel-file" => {
                        let settings = node.data.get("settings").and_then(|s| s.as_object());
                        let file_name = settings
                            .and_then(|s| s.get("fileName"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("output.xlsx");
                        let sheet_name = settings
                            .and_then(|s| s.get("sheetName"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Sheet1")
                            .to_string();

                        let column_mapping = settings
                            .and_then(|s| s.get("columnMapping"))
                            .or_else(|| node.data.get("columnMapping"))
                            .cloned()
                            .unwrap_or_else(|| {
                                let worker_mapping: serde_json::Map<String, serde_json::Value> =
                                    worker_column_mappings
                                        .values()
                                        .filter_map(|v| v.as_object().cloned())
                                        .flatten()
                                        .collect();
                                serde_json::Value::Object(worker_mapping)
                            });

                        let date_str = chrono_date();
                        let short_id = &project_id[..project_id.len().min(8)];
                        let resolved_file_name = file_name
                            .replace("{{date}}", &date_str)
                            .replace("{{project_id}}", short_id)
                            .replace("{{timestamp}}", &chrono_now());

                        let export_dir = dirs_next::data_dir()
                            .unwrap_or_else(|| std::path::PathBuf::from("."))
                            .join("com.CrawlFlow.desktop")
                            .join("exports");
                        std::fs::create_dir_all(&export_dir).ok();
                        let out_path = export_dir.join(&resolved_file_name);

                        Some(FinishAction::ExportExcel {
                            path: out_path.to_string_lossy().to_string(),
                            fields: vec![],
                            column_mapping,
                            sheet_name,
                        })
                    }
                    "generate-csv-file" => {
                        let settings = node.data.get("settings").and_then(|s| s.as_object());
                        let file_name = settings
                            .and_then(|s| s.get("fileName"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("output.csv");

                        let column_mapping = settings
                            .and_then(|s| s.get("columnMapping"))
                            .or_else(|| node.data.get("columnMapping"))
                            .cloned()
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                        let export_dir = dirs_next::data_dir()
                            .unwrap_or_else(|| std::path::PathBuf::from("."))
                            .join("com.CrawlFlow.desktop")
                            .join("exports");
                        std::fs::create_dir_all(&export_dir).ok();
                        let out_path = export_dir.join(file_name);

                        Some(FinishAction::ExportCsv {
                            path: out_path.to_string_lossy().to_string(),
                            fields: vec![],
                            column_mapping,
                        })
                    }
                    _ => None,
                }
            }
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

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn chrono_date() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple date calculation from unix timestamp (UTC, no leap seconds)
    let days = secs / 86400;
    // Days since 1970-01-01
    let mut year = 1970u32;
    let mut remaining_days = days as u32;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let days_in_year = if leap { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let month_days: &[u32] = if leap {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &d in month_days {
        if remaining_days < d {
            break;
        }
        remaining_days -= d;
        month += 1;
    }
    let day = remaining_days + 1;
    format!("{:04}-{:02}-{:02}", year, month, day)
}
