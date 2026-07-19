use crate::logs::LogManager;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub project_id: String,
    pub status: String,
    pub cycle_count: u64,
    pub started_at: String,
    pub last_run_at: String,
    pub last_error: Option<String>,
    pub interval_seconds: u64,
    // Realtime progress (written by the background service each cycle)
    pub progress: ServiceProgress,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServiceProgress {
    pub items_total: i64,
    pub items_processed: i64,
    pub items_success: i64,
    pub items_failed: i64,
    pub items_pending: i64,
    pub progress_pct: f64,
    pub phase: String,
    pub message: String,
    pub last_run_at: String,
}

impl Default for ServiceInfo {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            status: "stopped".to_string(),
            cycle_count: 0,
            started_at: String::new(),
            last_run_at: String::new(),
            last_error: None,
            interval_seconds: 60,
            progress: ServiceProgress::default(),
        }
    }
}

// Simplified ServiceManager - only reads/writes SQLite state
// All execution happens in background service (bin/service.rs)

pub struct ServiceManager {
    app_handle: RwLock<Option<AppHandle>>,
    log_manager: RwLock<Option<Arc<LogManager>>>,
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        if let Ok(mut child) = std::process::Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            if let Ok(status) = child.wait() {
                return status.success();
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(output) = std::process::Command::new("tasklist")
            .arg("/FI")
            .arg(format!("PID eq {}", pid))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout.contains(&pid.to_string());
        }
    }
    false
}

fn is_project_running_in_background(project_id: &str) -> bool {
    let path = dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.CrawlFlow.desktop")
        .join(format!("{}.run", project_id));
    if !path.exists() {
        return false;
    }
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            return is_process_running(pid);
        }
    }
    false
}

impl ServiceManager {
    pub fn new_uninitialized() -> Self {
        Self {
            app_handle: RwLock::new(None),
            log_manager: RwLock::new(None),
        }
    }

    pub fn initialize(&self, app_handle: AppHandle, log_manager: Arc<LogManager>) {
        *self.app_handle.write().unwrap() = Some(app_handle);
        *self.log_manager.write().unwrap() = Some(log_manager);
    }

    fn lm(&self) -> Arc<LogManager> {
        self.log_manager
            .read()
            .unwrap()
            .clone()
            .expect("ServiceManager not initialized")
    }

    pub fn start_service(
        &self,
        project_id: &str,
        _nodes: Vec<serde_json::Value>,
        _edges: Vec<serde_json::Value>,
        _settings: serde_json::Value,
    ) -> Result<(), String> {
        // Only write service_control to SQLite - background service handles execution
        let db_path = dirs_next::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.CrawlFlow.desktop")
            .join("crawlflow.db");
        
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            conn.execute(
                "INSERT INTO project_runtime (project_id, service_control, runner_status, updated_at)
                 VALUES (?1, 'run', 'running', datetime('now'))
                 ON CONFLICT(project_id) DO UPDATE SET service_control = 'run', runner_status = 'running', updated_at = datetime('now')",
                rusqlite::params![project_id],
            ).map_err(|e| e.to_string())?;
        }
        
        let _ = self.lm().info(project_id, "system", "Service start requested");
        Ok(())
    }


    pub fn stop_service(&self, project_id: &str) -> Result<(), String> {
        let db_path = dirs_next::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.CrawlFlow.desktop")
            .join("crawlflow.db");
        
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            conn.execute(
                "INSERT INTO project_runtime (project_id, service_control, runner_status, updated_at)
                 VALUES (?1, 'stop', 'stopped', datetime('now'))
                 ON CONFLICT(project_id) DO UPDATE SET service_control = 'stop', runner_status = 'stopped', updated_at = datetime('now')",
                rusqlite::params![project_id],
            ).map_err(|e| e.to_string())?;
        }
        
        let _ = self.lm().info(project_id, "system", "Service stop requested");
        Ok(())
    }

    pub fn pause_service(&self, project_id: &str) -> Result<(), String> {
        let db_path = dirs_next::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.CrawlFlow.desktop")
            .join("crawlflow.db");
        
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            conn.execute(
                "INSERT INTO project_runtime (project_id, service_control, runner_status, updated_at)
                 VALUES (?1, 'paused', 'paused', datetime('now'))
                 ON CONFLICT(project_id) DO UPDATE SET service_control = 'paused', runner_status = 'paused', updated_at = datetime('now')",
                rusqlite::params![project_id],
            ).map_err(|e| e.to_string())?;
        }
        
        let _ = self.lm().info(project_id, "system", "Service pause requested");
        Ok(())
    }

    pub fn resume_service(&self, project_id: &str) -> Result<(), String> {
        let db_path = dirs_next::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.CrawlFlow.desktop")
            .join("crawlflow.db");
        
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            conn.execute(
                "INSERT INTO project_runtime (project_id, service_control, runner_status, updated_at)
                 VALUES (?1, 'run', 'running', datetime('now'))
                 ON CONFLICT(project_id) DO UPDATE SET service_control = 'run', runner_status = 'running', updated_at = datetime('now')",
                rusqlite::params![project_id],
            ).map_err(|e| e.to_string())?;
        }
        
        let _ = self.lm().info(project_id, "system", "Service resume requested");
        Ok(())
    }

    pub fn get_service_info(&self, project_id: &str) -> Option<ServiceInfo> {
        // Read only from SQLite project_runtime (background service state)
        let db_path = dirs_next::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.CrawlFlow.desktop")
            .join("crawlflow.db");
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            Self::ensure_progress_column(&conn);
            let row: rusqlite::Result<(String, i64, Option<String>, Option<String>, Option<String>)> = conn.query_row(
                "SELECT runner_status, cycle_count, last_run_at, last_error, progress_json FROM project_runtime WHERE project_id = ?1",
                rusqlite::params![project_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4).ok().flatten())),
            );
            if let Ok((status, cycle_count, last_run_at, last_error, progress_json)) = row {
                // For background service "running" status, verify the PID is still alive
                let effective_status = if status == "running" {
                    if is_project_running_in_background(project_id) {
                        "running"
                    } else {
                        "stopped"
                    }
                } else {
                    &status
                };
                return Some(ServiceInfo {
                    project_id: project_id.to_string(),
                    status: effective_status.to_string(),
                    cycle_count: cycle_count as u64,
                    started_at: String::new(),
                    last_run_at: last_run_at.clone().unwrap_or_default(),
                    last_error,
                    interval_seconds: 60,
                    progress: Self::parse_progress(progress_json.as_deref(), last_run_at),
                });
            }
        }

        // Default: stopped (no record means no service has ever run)
        Some(ServiceInfo::default())
    }

    pub fn list_service_infos(&self) -> Vec<ServiceInfo> {
        // Read from SQLite - list all projects with runtime info
        let db_path = dirs_next::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.CrawlFlow.desktop")
            .join("crawlflow.db");

        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        Self::ensure_progress_column(&conn);

        let mut stmt = match conn.prepare("SELECT project_id, runner_status, cycle_count, last_run_at, last_error, progress_json FROM project_runtime") {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        let rows = match stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return vec![],
        };

        rows.filter_map(|r| r.ok())
            .map(|(pid, status, cycle_count, last_run_at, last_error, progress_json)| {
                let effective_status = if status == "running" {
                    if is_project_running_in_background(&pid) {
                        "running"
                    } else {
                        "stopped"
                    }
                } else {
                    &status
                };
                ServiceInfo {
                    project_id: pid,
                    status: effective_status.to_string(),
                    cycle_count: cycle_count as u64,
                    started_at: String::new(),
                    last_run_at: last_run_at.clone().unwrap_or_default(),
                    last_error,
                    interval_seconds: 60,
                    progress: Self::parse_progress(progress_json.as_deref(), last_run_at),
                }
            })
            .collect()
    }

    fn ensure_progress_column(conn: &rusqlite::Connection) {
        let _ = conn.execute(
            "ALTER TABLE project_runtime ADD COLUMN progress_json TEXT",
            [],
        );
    }

    fn parse_progress(progress_json: Option<&str>, last_run_at: Option<String>) -> ServiceProgress {
        match progress_json {
            Some(s) if !s.is_empty() => {
                serde_json::from_str::<ServiceProgress>(s).unwrap_or_else(|_| {
                    let mut p = ServiceProgress::default();
                    p.last_run_at = last_run_at.unwrap_or_default();
                    p
                })
            }
            _ => {
                let mut p = ServiceProgress::default();
                p.last_run_at = last_run_at.unwrap_or_default();
                p
            }
        }
    }

    /// Persist realtime progress JSON for a project into the master DB.
    /// Called by the background service each cycle.
    pub fn write_progress_json(project_id: &str, progress: &ServiceProgress) {
        ensure_progress_column_static();
        if let Ok(conn) = rusqlite::Connection::open(master_db_path()) {
            Self::ensure_progress_column(&conn);
            let json = serde_json::to_string(progress).unwrap_or_default();
            let _ = conn.execute(
                "INSERT INTO project_runtime (project_id, progress_json, updated_at)
                 VALUES (?1, ?2, datetime('now'))
                 ON CONFLICT(project_id) DO UPDATE SET progress_json = ?2, updated_at = datetime('now')",
                rusqlite::params![project_id, json],
            );
        }
    }
}

fn master_db_path() -> std::path::PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.CrawlFlow.desktop")
        .join("crawlflow.db")
}

fn ensure_progress_column_static() {
    if let Ok(conn) = rusqlite::Connection::open(master_db_path()) {
        let _ = conn.execute("ALTER TABLE project_runtime ADD COLUMN progress_json TEXT", []);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_info_construction() {
        let info = ServiceInfo {
            project_id: "test-proj".into(),
            status: "running".into(),
            cycle_count: 5,
            started_at: "2024-01-01T00:00:00.000Z".into(),
            last_run_at: "2024-01-01T01:00:00.000Z".into(),
            last_error: None,
            interval_seconds: 60,
            progress: ServiceProgress::default(),
        };
        assert_eq!(info.project_id, "test-proj");
        assert_eq!(info.status, "running");
        assert_eq!(info.cycle_count, 5);
        assert!(info.last_error.is_none());
    }

    #[test]
    fn test_service_info_with_error() {
        let info = ServiceInfo {
            project_id: "p".into(),
            status: "error: timeout".into(),
            cycle_count: 1,
            started_at: String::new(),
            last_run_at: String::new(),
            last_error: Some("timeout".into()),
            interval_seconds: 30,
            progress: ServiceProgress::default(),
        };
        assert_eq!(info.status, "error: timeout");
        assert_eq!(info.last_error.unwrap(), "timeout");
    }

    #[test]
    fn test_service_info_serde_roundtrip() {
        let info = ServiceInfo {
            project_id: "proj-1".into(),
            status: "running".into(),
            cycle_count: 5,
            started_at: "2026-01-01T00:00:00.000Z".into(),
            last_run_at: "2026-01-02T00:00:00.000Z".into(),
            last_error: None,
            interval_seconds: 60,
            progress: ServiceProgress::default(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ServiceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.project_id, "proj-1");
        assert_eq!(back.status, "running");
        assert_eq!(back.cycle_count, 5);
        assert_eq!(back.interval_seconds, 60);
        assert!(back.last_error.is_none());
    }

    #[test]
    fn test_service_info_with_last_error() {
        let info = ServiceInfo {
            project_id: "proj-2".into(),
            status: "error: timeout".into(),
            cycle_count: 3,
            started_at: "2026-01-01T00:00:00.000Z".into(),
            last_run_at: "2026-01-03T00:00:00.000Z".into(),
            last_error: Some("timeout".into()),
            interval_seconds: 30,
            progress: ServiceProgress::default(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ServiceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, "error: timeout");
        assert_eq!(back.last_error, Some("timeout".into()));
        assert_eq!(back.interval_seconds, 30);
    }
}
