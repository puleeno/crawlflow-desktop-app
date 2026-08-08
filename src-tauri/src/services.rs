use crate::logs::LogManager;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Emitter};

lazy_static! {
    /// Global in-memory status cache. Key = project_id.
    /// Updated immediately on every status change; broadcast thread reads from here.
    static ref SERVICE_STATE: Mutex<HashMap<String, ServiceInfo>> = Mutex::new(HashMap::new());
}

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
    // Port of the project's realtime WebSocket server (0 = not running)
    pub ws_port: u16,
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

/// Per-run export configuration resolved from the global `app_settings`
/// (`export_dir`) and the per-project `project_settings` (`group_export`,
/// `group_format`). Passed into the export plugin so output files are placed
/// in the user's chosen folder and (optionally) grouped per project.
#[derive(Debug, Clone, Default)]
pub struct ExportSettings {
    /// Global export directory from `app_settings.export_dir`. `None` means
    /// "use the OS Downloads folder".
    pub export_dir: Option<String>,
    /// Whether to create a per-project subfolder.
    pub group_export: bool,
    /// Subfolder label format: "id" | "name" | "both".
    pub group_format: String,
}

fn master_db_path() -> std::path::PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.CrawlFlow.desktop")
        .join("crawlflow.db")
}

/// Read the global `export_dir` from `app_settings`. If empty and not yet
/// scanned, auto-detect the OS Downloads folder, save it, and set the
/// scanned flag so the slow scan runs only once.
/// 
/// When running as a Windows Service, the override parameter (passed via
/// --export-dir at service install time) takes precedence to ensure files
/// are written to the user's Downloads folder instead of systemprofile.
pub fn read_global_export_dir_with_override(export_dir_override: Option<String>) -> Option<String> {
    // If service provided an override (e.g., user's Downloads), use it directly
    if let Some(override_dir) = export_dir_override {
        let trimmed = override_dir.trim().to_string();
        if !trimmed.is_empty() {
            println!("[Export] Using export dir override: {}", trimmed);
            return Some(trimmed);
        }
    }

    let conn = rusqlite::Connection::open(master_db_path()).ok()?;

    // 1. Check if already set by user
    let existing: Option<String> = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'export_dir'",
            [],
            |row| row.get(0),
        )
        .ok()
        .and_then(|v: String| {
            let trimmed = v.trim().to_string();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        });

    if existing.is_some() {
        return existing;
    }

    // 2. Check if already scanned (skip if we already tried once)
    let already_scanned: bool = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'export_dir_scanned'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);

    if already_scanned {
        return None;
    }

    // 3. First scan: auto-detect Downloads folder
    let detected = dirs_next::download_dir()
        .or_else(|| dirs_next::data_dir())
        .and_then(|p| {
            let s = p.to_string_lossy().to_string();
            if s.is_empty() { None } else { Some(s) }
        });

    // 4. Save result and scanned flag
    if let Some(ref path) = detected {
        let _ = conn.execute(
            "INSERT INTO app_settings (key, value) VALUES ('export_dir', ?1) ON CONFLICT(key) DO UPDATE SET value = ?1",
            rusqlite::params![path],
        );
        println!("[Export] Auto-detected Downloads folder: {}", path);
    }
    let _ = conn.execute(
        "INSERT INTO app_settings (key, value) VALUES ('export_dir_scanned', '1') ON CONFLICT(key) DO UPDATE SET value = '1'",
        [],
    );

    detected
}

/// Read the global `export_dir` from `app_settings` (no override).
pub fn read_global_export_dir() -> Option<String> {
    read_global_export_dir_with_override(None)
}

/// Read `group_export` / `group_format` from the per-project `project_settings`
/// table. Returns defaults (enabled, "name") when the project DB is unavailable.
fn read_project_export_settings(project_db_path: &Path) -> (bool, String) {
    let conn = rusqlite::Connection::open(project_db_path).ok();
    let conn = match conn {
        Some(c) => c,
        None => return (true, "name".to_string()),
    };
    let get = |key: &str| -> Option<String> {
        conn.query_row(
            "SELECT value FROM project_settings WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .ok()
    };
    let group_export = match get("group_export").as_deref() {
        Some("false") | Some("0") => false,
        _ => true,
    };
    let group_format = match get("group_format").as_deref() {
        Some("id") => "id".to_string(),
        Some("both") => "both".to_string(),
        _ => "name".to_string(),
    };
    (group_export, group_format)
}

/// Read refresh strategy from the per-project `project_settings`.
/// Returns the strategy string and optional update method / interval.
pub fn read_project_refresh_strategy(project_db_path: &Path) -> (String, String, u64) {
    let conn = match rusqlite::Connection::open(project_db_path).ok() {
        Some(c) => c,
        None => return ("update_only".into(), "check_first_page_until_duplicate".into(), 3600),
    };
    let get = |key: &str| -> Option<String> {
        conn.query_row(
            "SELECT value FROM project_settings WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .ok()
    };
    let strategy = get("refresh_strategy")
        .filter(|s| matches!(s.as_str(), "refresh" | "refresh_update" | "update_only"))
        .unwrap_or_else(|| "update_only".into());
    let update_method = get("update_method")
        .filter(|s| matches!(s.as_str(), "check_last_page" | "check_first_page_until_duplicate"))
        .unwrap_or_else(|| "check_first_page_until_duplicate".into());
    let interval = get("refresh_interval")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(3600)
        .max(60);
    (strategy, update_method, interval)
}

/// Resolve the full export settings for a project run.
/// When running as a service, the export_dir_override (user's Downloads)
/// should be passed to avoid writing to systemprofile.
pub fn get_export_settings_with_override(
    project_id: &str, 
    project_db_path: &Path,
    export_dir_override: Option<String>
) -> ExportSettings {
    let export_dir = read_global_export_dir_with_override(export_dir_override);
    let (group_export, group_format) = read_project_export_settings(project_db_path);
    let _ = project_id;
    ExportSettings {
        export_dir,
        group_export,
        group_format,
    }
}

/// Resolve the full export settings for a project run (no override).
pub fn get_export_settings(project_id: &str, project_db_path: &Path) -> ExportSettings {
    get_export_settings_with_override(project_id, project_db_path, None)
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
            ws_port: 0,
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
        extern "system" {
            fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
            fn CloseHandle(hObject: isize) -> i32;
        }
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle != 0 {
                CloseHandle(handle);
                return true;
            }
        }
    }
    false
}

fn is_project_running_in_background(project_id: &str) -> bool {
    // The background service (bin/service.rs) records its live PID in the
    // `runner_pid` column of `project_runtime` on every status write, so we
    // verify liveness from there instead of relying on a `.run` pidfile that
    // the service never writes. Falling back to a non-existent file caused the
    // GUI to show `stopped` (and hide the progress bar) even while crawling.
    let db_path = dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.CrawlFlow.desktop")
        .join("crawlflow.db");
    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
        let pid: rusqlite::Result<Option<i64>> = conn.query_row(
            "SELECT runner_pid FROM project_runtime WHERE project_id = ?1",
            rusqlite::params![project_id],
            |row| row.get(0),
        );
        if let Ok(Some(pid)) = pid {
            if pid > 0 {
                return is_process_running(pid as u32);
            }
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
        *self.app_handle.write().unwrap() = Some(app_handle.clone());
        *self.log_manager.write().unwrap() = Some(log_manager);
        // Load initial state from SQLite into RAM
        Self::sync_sqlite_to_ram();
        // Start broadcast thread (reads from RAM, emits Tauri events)
        self.start_status_broadcast(app_handle.clone());
        // Start sync thread (writes RAM to SQLite every 2s)
        Self::start_ram_to_sqlite_sync();
    }

    /// Load all project runtime info from SQLite into the in-memory state.
    fn sync_sqlite_to_ram() {
        let infos = Self::read_all_runtime_from_sqlite();
        let mut state = SERVICE_STATE.lock().unwrap();
        for info in infos {
            state.insert(info.project_id.clone(), info);
        }
    }

    /// Spawn a background thread that syncs RAM -> SQLite every 2 seconds.
    /// This is the ONLY place that writes status to SQLite (besides initial load).
    fn start_ram_to_sqlite_sync() {
        std::thread::spawn(|| loop {
            std::thread::sleep(std::time::Duration::from_millis(2000));
            let snapshot: Vec<ServiceInfo> = {
                let state = SERVICE_STATE.lock().unwrap();
                state.values().cloned().collect()
            };
            if snapshot.is_empty() {
                continue;
            }
            let db_path = dirs_next::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("com.CrawlFlow.desktop")
                .join("crawlflow.db");
            if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=3000;");
                for info in &snapshot {
                    let progress_json = serde_json::to_string(&info.progress).unwrap_or_default();
                    let _ = conn.execute(
                        "INSERT INTO project_runtime (project_id, runner_status, cycle_count, last_run_at, last_error, progress_json, ws_port, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
                         ON CONFLICT(project_id) DO UPDATE SET
                             runner_status = ?2, cycle_count = ?3, last_run_at = ?4,
                             last_error = ?5, progress_json = ?6, ws_port = ?7, updated_at = datetime('now')",
                        rusqlite::params![
                            info.project_id, info.status, info.cycle_count as i64,
                            info.last_run_at, info.last_error, progress_json, info.ws_port as i64,
                        ],
                    );
                }
            }
        });
    }

    /// Broadcast thread: reads from RAM (instant) and emits Tauri events every 100ms.
    fn start_status_broadcast(&self, app_handle: AppHandle) {
        std::thread::spawn(move || loop {
            let snapshot: Vec<ServiceInfo> = {
                let state = SERVICE_STATE.lock().unwrap();
                state.values().cloned().collect()
            };
            for info in &snapshot {
                let event = format!("service-status:{}", info.project_id);
                let _ = app_handle.emit(&event, info);
                let _ = app_handle.emit(
                    "service-status-update",
                    serde_json::json!({ "project_id": info.project_id, "info": info }),
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        });
    }

    /// Update status in RAM immediately. Called by GUI actions (start/stop/pause).
    /// Also emits Tauri event right away — no waiting for broadcast thread.
    pub fn update_status_immediate(
        project_id: &str,
        status: &str,
        progress: Option<ServiceProgress>,
        app_handle: &AppHandle,
    ) {
        let info = {
            let mut state = SERVICE_STATE.lock().unwrap();
            let entry = state.entry(project_id.to_string()).or_insert_with(|| ServiceInfo {
                project_id: project_id.to_string(),
                status: status.to_string(),
                ..ServiceInfo::default()
            });
            entry.status = status.to_string();
            if let Some(p) = progress {
                entry.progress = p;
            }
            entry.clone()
        };
        // Emit immediately — no need to wait for broadcast thread
        let event = format!("service-status:{}", project_id);
        let _ = app_handle.emit(&event, &info);
        let _ = app_handle.emit(
            "service-status-update",
            serde_json::json!({ "project_id": project_id, "info": info }),
        );
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
        // Check if project name is empty before starting service
        let project_db_path = dirs_next::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.CrawlFlow.desktop")
            .join(format!("project_{}.db", project_id));
        
        if let Ok(conn) = rusqlite::Connection::open(&project_db_path) {
            let project_name: Result<String, _> = conn.query_row(
                "SELECT value FROM project_settings WHERE key = 'name'",
                [],
                |row| row.get(0),
            );
            
            if let Ok(name) = project_name {
                if name.trim().is_empty() {
                    return Err("Project name cannot be empty. Please set a project name before starting the service.".to_string());
                }
            }
        }
        
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
        // Read from RAM (instant, no DB access)
        let state = SERVICE_STATE.lock().unwrap();
        state.get(project_id).cloned()
    }

    /// Update progress in RAM immediately. Called when WebSocket receives
    /// progress frame from background service.
    pub fn update_progress_immediate(project_id: &str, progress: ServiceProgress) {
        let mut state = SERVICE_STATE.lock().unwrap();
        let entry = state.entry(project_id.to_string()).or_insert_with(|| ServiceInfo {
            project_id: project_id.to_string(),
            ..ServiceInfo::default()
        });
        entry.progress = progress;
    }

    /// Broadcast a `ServiceInfo` to the frontend.
    ///
    /// Emits two events (Tauri v2 has no wildcard listeners):
    /// * `service-status:<project_id>` — for the per-project detail views.
    /// * `service-status-update` (payload `{ project_id, info }`) — for the
    ///   global project list, which can't subscribe per-id in advance.
    fn emit_service_info(&self, info: &ServiceInfo) {
        if let Some(handle) = self.app_handle.read().ok().and_then(|h| h.clone()) {
            let project_id = info.project_id.clone();
            let event = format!("service-status:{}", project_id);
            let _ = handle.emit(&event, info);
            let _ = handle.emit(
                "service-status-update",
                serde_json::json!({ "project_id": project_id, "info": info }),
            );
        }
    }

    /// Read every project's runtime info from RAM (instant, no DB access).
    /// Used by the status broadcast thread and list_service_infos.
    pub fn read_all_runtime() -> Vec<ServiceInfo> {
        let state = SERVICE_STATE.lock().unwrap();
        state.values().cloned().collect()
    }

    /// Read from SQLite (used only for initial load into RAM).
    fn read_all_runtime_from_sqlite() -> Vec<ServiceInfo> {
        let db_path = dirs_next::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("com.CrawlFlow.desktop")
            .join("crawlflow.db");
        Self::read_all_runtime_from(&db_path)
    }

    fn read_all_runtime_from(db_path: &std::path::Path) -> Vec<ServiceInfo> {
        let conn = match rusqlite::Connection::open(db_path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        Self::ensure_progress_column(&conn);

        let mut stmt = match conn.prepare("SELECT project_id, runner_status, cycle_count, last_run_at, last_error, progress_json, ws_port FROM project_runtime") {
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
                row.get::<_, Option<i64>>(6)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return vec![],
        };

        rows.filter_map(|r| r.ok())
            .map(|(pid, status, cycle_count, last_run_at, last_error, progress_json, ws_port)| {
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
                    ws_port: ws_port.unwrap_or(0) as u16,
                }
            })
            .collect()
    }

    pub fn list_service_infos(&self) -> Vec<ServiceInfo> {
        let infos = Self::read_all_runtime();
        for info in &infos {
            self.emit_service_info(info);
        }
        infos
    }

    fn ensure_progress_column(conn: &rusqlite::Connection) {
        let _ = conn.execute(
            "ALTER TABLE project_runtime ADD COLUMN progress_json TEXT",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE project_runtime ADD COLUMN ws_port INTEGER",
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

fn ensure_progress_column_static() {
    if let Ok(conn) = rusqlite::Connection::open(master_db_path()) {
        let _ = conn.execute("ALTER TABLE project_runtime ADD COLUMN progress_json TEXT", []);
        let _ = conn.execute("ALTER TABLE project_runtime ADD COLUMN ws_port INTEGER", []);
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
            ws_port: 0,
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
            ws_port: 0,
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
            ws_port: 0,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ServiceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.project_id, "proj-1");
        assert_eq!(back.status, "running");
        assert_eq!(back.cycle_count, 5);
        assert_eq!(back.interval_seconds, 60);
        assert_eq!(back.ws_port, 0);
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
            ws_port: 0,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ServiceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, "error: timeout");
        assert_eq!(back.last_error.unwrap(), "timeout");
        assert_eq!(back.interval_seconds, 30);
        assert_eq!(back.ws_port, 0);
    }
}
