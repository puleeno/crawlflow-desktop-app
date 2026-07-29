//! Logging subsystem — Monolog-style handler pipeline.
//!
//! `LogManager` is the central logger. Every `emit()` fans the `LogEntry` out
//! to a list of pluggable `LogHandler`s (channels), each responsible for ONE
//! destination:
//!
//!   * `WsLogHandler`    — pushes the entry to connected WebSocket clients
//!                          (realtime GUI feed, zero polling delay).
//!   * `DbLogHandler`    — enqueues the entry on an mpsc channel; a dedicated
//!                          background thread persists it to SQLite so the
//!                          emit() path is never blocked on disk I/O.
//!   * `TauriLogHandler` — emits a Tauri event for in-process (GUI) execution.
//!   * `BufferLogHandler`— keeps an in-memory ring buffer for fast reads.
//!
//! Handlers run independently: the DB write happens on its own thread, the WS
//! push is a non-blocking broadcast send, so logging stays cheap and realtime.

use crate::ws::{WsHub, WsMessage};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// Global reference to the active LogManager and the project currently running,
/// so that Python plugin logs (via `crawlflow.log`) can be routed to the active
/// logger instead of being lost to the Rust logger only.
static ACTIVE_LOG_CONTEXT: LazyLock<RwLock<Option<(Arc<LogManager>, String)>>> =
    LazyLock::new(|| RwLock::new(None));

/// Register the LogManager + project id that Python plugin logs should route to.
/// Called at the start of a pipeline run.
pub fn set_active_log_context(log_manager: Arc<LogManager>, project_id: &str) {
    *ACTIVE_LOG_CONTEXT.write().unwrap() = Some((log_manager, project_id.to_string()));
}

/// Clear the active log context at the end of a pipeline run.
pub fn clear_active_log_context() {
    *ACTIVE_LOG_CONTEXT.write().unwrap() = None;
}

pub struct LogContextGuard;

impl LogContextGuard {
    pub fn new(log_manager: Arc<LogManager>, project_id: &str) -> Self {
        set_active_log_context(log_manager, project_id);
        Self
    }
}

impl Drop for LogContextGuard {
    fn drop(&mut self) {
        clear_active_log_context();
    }
}

/// Route a log message coming from a Python plugin to the active LogManager.
/// Falls back to the Rust logger when no pipeline context is active.
pub fn log_from_plugin(level: &str, message: &str) {
    if let Some((ref lm, ref project_id)) = *ACTIVE_LOG_CONTEXT.read().unwrap() {
        lm.emit(project_id, level, "PythonPlugin", message, None);
    } else {
        log::info!("[PythonPlugin] [{}] {}", level, message);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: u64,
    pub project_id: String,
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatusPayload {
    pub project_id: String,
    pub status: String,
    pub cycle_count: u64,
    pub started_at: String,
    pub last_run_at: String,
    pub last_error: Option<String>,
}

const MAX_LOG_ENTRIES: usize = 5000;

/// A log destination. Implementors receive every emitted entry.
pub trait LogHandler: Send + Sync {
    fn handle(&self, entry: &LogEntry);
}

/// In-memory ring buffer (fast synchronous reads, used by GUI in-process runs).
struct BufferLogHandler {
    buffer: Arc<RwLock<VecDeque<LogEntry>>>,
}

impl LogHandler for BufferLogHandler {
    fn handle(&self, entry: &LogEntry) {
        let mut buf = self.buffer.write().unwrap();
        if buf.len() >= MAX_LOG_ENTRIES {
            buf.pop_front();
        }
        buf.push_back(entry.clone());
    }
}

/// Realtime WebSocket fan-out. Non-blocking broadcast send — never stalls the
/// emitter. Only active in the headless service where a global WS hub exists.
struct WsLogHandler {
    hub: Arc<WsHub>,
}

impl LogHandler for WsLogHandler {
    fn handle(&self, entry: &LogEntry) {
        self.hub.publish(
            &entry.project_id,
            &WsMessage::log(serde_json::json!({
                "id": entry.id,
                "timestamp": entry.timestamp,
                "level": entry.level,
                "source": entry.source,
                "message": entry.message,
                "details": entry.details,
            })),
        );
    }
}

/// Tauri event fan-out (only meaningful in the GUI process with an AppHandle).
struct TauriLogHandler {
    handle: AppHandle,
}

impl LogHandler for TauriLogHandler {
    fn handle(&self, entry: &LogEntry) {
        let event_name = format!("project-log:{}", entry.project_id);
        let _ = self.handle.emit(&event_name, entry);
    }
}

/// SQLite persistence on a dedicated background thread.
///
/// `emit()` only does a cheap `tx.send()` onto the shared `DB_SENDER` channel;
/// a spawned worker (started once in `install_*_handlers`) drains it and writes
/// to the DB. This decouples logging throughput from disk latency and keeps the
/// realtime WS path unblocked.
struct DbLogHandler;

// The DbLogHandler shares a single mpsc sender across all instances. The
// sender + its worker thread are created once (lazily) in `install_*_handlers`.
static DB_SENDER: LazyLock<Mutex<Option<Sender<LogEntry>>>> = LazyLock::new(|| Mutex::new(None));

impl LogHandler for DbLogHandler {
    fn handle(&self, entry: &LogEntry) {
        if let Some(tx) = DB_SENDER.lock().unwrap().as_ref() {
            let _ = tx.send(entry.clone());
        }
    }
}

/// The central logger. Holds an ordered list of handlers (channels).
pub struct LogManager {
    handlers: RwLock<Vec<Box<dyn LogHandler>>>,
    next_id: Arc<AtomicU64>,
    buffer: Arc<RwLock<VecDeque<LogEntry>>>,
    master_db_path: Mutex<Option<PathBuf>>,
    ws_hub: RwLock<Option<Arc<WsHub>>>,
}

impl LogManager {
    pub fn new() -> Self {
        let buffer = Arc::new(RwLock::new(VecDeque::with_capacity(MAX_LOG_ENTRIES)));
        Self {
            // The ring buffer is always attached so emits are readable in-memory
            // even before install_service_handlers / install_gui_handlers runs.
            handlers: RwLock::new(vec![Box::new(BufferLogHandler { buffer: buffer.clone() })]),
            next_id: Arc::new(AtomicU64::new(1)),
            buffer,
            master_db_path: Mutex::new(None),
            ws_hub: RwLock::new(None),
        }
    }

    /// Install the standard handler stack for a headless service run:
    /// ring buffer + WebSocket realtime + async DB persistence.
    pub fn install_service_handlers(&self, db_path: PathBuf) {
        self.ensure_logs_table(&db_path);
        *self.master_db_path.lock().unwrap() = Some(db_path.clone());

        // Lazily spin up the shared DB writer thread + sender once.
        {
            let mut guard = DB_SENDER.lock().unwrap();
            if guard.is_none() {
                let (tx, rx): (Sender<LogEntry>, _) = channel::<LogEntry>();
                let worker_db = db_path.clone();
                std::thread::spawn(move || {
                    let conn = rusqlite::Connection::open(&worker_db).ok();
                    while let Ok(entry) = rx.recv() {
                        if let Some(ref conn) = conn {
                            let _ = conn.execute(
                                "INSERT INTO logs (project_id, timestamp, level, source, message, details) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                                params![
                                    entry.project_id, entry.timestamp, entry.level,
                                    entry.source, entry.message, entry.details
                                ],
                            );
                        }
                    }
                });
                *guard = Some(tx);
            }
        }

        let mut handlers = self.handlers.write().unwrap();
        handlers.clear();
        handlers.push(Box::new(BufferLogHandler { buffer: self.buffer.clone() }));
        if let Some(hub) = self.ws_hub.read().unwrap().clone() {
            handlers.push(Box::new(WsLogHandler { hub }));
        }
        handlers.push(Box::new(DbLogHandler));
    }

    /// Install the standard handler stack for the GUI process:
    /// ring buffer + Tauri event (in-process) + async DB persistence.
    pub fn install_gui_handlers(&self, app_handle: AppHandle, db_path: PathBuf) {
        self.ensure_logs_table(&db_path);
        *self.master_db_path.lock().unwrap() = Some(db_path.clone());

        {
            let mut guard = DB_SENDER.lock().unwrap();
            if guard.is_none() {
                let (tx, rx): (Sender<LogEntry>, _) = channel::<LogEntry>();
                let worker_db = db_path.clone();
                std::thread::spawn(move || {
                    let conn = rusqlite::Connection::open(&worker_db).ok();
                    while let Ok(entry) = rx.recv() {
                        if let Some(ref conn) = conn {
                            let _ = conn.execute(
                                "INSERT INTO logs (project_id, timestamp, level, source, message, details) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                                params![
                                    entry.project_id, entry.timestamp, entry.level,
                                    entry.source, entry.message, entry.details
                                ],
                            );
                        }
                    }
                });
                *guard = Some(tx);
            }
        }

        let mut handlers = self.handlers.write().unwrap();
        handlers.clear();
        handlers.push(Box::new(BufferLogHandler { buffer: self.buffer.clone() }));
        handlers.push(Box::new(TauriLogHandler { handle: app_handle }));
        handlers.push(Box::new(DbLogHandler));
    }

    /// Register an extra handler (e.g. a WebSocket handler added after init).
    pub fn add_handler(&self, handler: Box<dyn LogHandler>) {
        self.handlers.write().unwrap().push(handler);
    }

    /// Attach the global WebSocket hub so a `WsLogHandler` can be wired.
    pub fn set_ws_hub(&self, hub: Arc<WsHub>) {
        *self.ws_hub.write().unwrap() = Some(hub.clone());
        // (Re)install service handlers if DB path is already known.
        let db = self.master_db_path.lock().unwrap().clone();
        if let Some(db) = db {
            self.install_service_handlers(db);
        }
    }

    /// Set the master DB path. Only stores the path + ensures the logs table;
    /// it does NOT (re)install handlers. Use `install_service_handlers` /
    /// `install_gui_handlers` / `set_app_handle` to build the handler stack.
    pub fn set_master_db_path(&self, path: PathBuf) {
        self.ensure_logs_table(&path);
        *self.master_db_path.lock().unwrap() = Some(path);
    }

    /// Set the Tauri AppHandle (legacy alias — installs GUI handlers).
    pub fn set_app_handle(&self, handle: AppHandle) {
        let db = self
            .master_db_path
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| PathBuf::from("crawlflow.db"));
        self.install_gui_handlers(handle, db);
    }

    /// Return the most recent non-debug log message, for use as a live
    /// progress message on the project card (e.g. "Processing item X…").
    pub fn latest_activity(&self) -> Option<String> {
        let buf = self.buffer.read().ok()?;
        // Walk backwards to find the newest info/warn/error entry
        for entry in buf.iter().rev() {
            if entry.level != "debug" {
                return Some(entry.message.clone());
            }
        }
        None
    }

    fn ensure_logs_table(&self, path: &PathBuf) {
        if let Ok(conn) = rusqlite::Connection::open(path) {
            let _ = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project_id TEXT NOT NULL,
                    timestamp TEXT NOT NULL,
                    level TEXT NOT NULL,
                    source TEXT NOT NULL,
                    message TEXT NOT NULL,
                    details TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_logs_project_id ON logs(project_id);
                CREATE INDEX IF NOT EXISTS idx_logs_timestamp ON logs(timestamp);",
            );
        }
    }

    fn now_iso() -> String {
        let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let secs = d.as_secs();
        let millis = d.subsec_millis();
        let days = (secs / 86400) as i64;
        let time_secs = secs % 86400;
        let hours = time_secs / 3600;
        let mins = (time_secs % 3600) / 60;
        let sec = time_secs % 60;
        let (year, month, day) = Self::civil_from_days(days);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            year, month, day, hours, mins, sec, millis
        )
    }

    fn civil_from_days(z: i64) -> (i64, u32, u32) {
        let z = z + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        (if m <= 2 { y + 1 } else { y }, m, d)
    }

    pub fn emit(
        &self,
        project_id: &str,
        level: &str,
        source: &str,
        message: &str,
        details: Option<String>,
    ) -> LogEntry {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let entry = LogEntry {
            id,
            project_id: project_id.to_string(),
            timestamp: Self::now_iso(),
            level: level.to_string(),
            source: source.to_string(),
            message: message.to_string(),
            details,
        };

        // Fan out to every handler (channel). WS + Tauri are non-blocking;
        // DB enqueues onto an mpsc channel consumed by a dedicated thread.
        for handler in self.handlers.read().unwrap().iter() {
            handler.handle(&entry);
        }

        entry
    }

    pub fn info(&self, project_id: &str, source: &str, message: &str) -> LogEntry {
        self.emit(project_id, "info", source, message, None)
    }

    pub fn warn(&self, project_id: &str, source: &str, message: &str) -> LogEntry {
        self.emit(project_id, "warn", source, message, None)
    }

    pub fn error(&self, project_id: &str, source: &str, message: &str) -> LogEntry {
        self.emit(project_id, "error", source, message, None)
    }

    pub fn debug(&self, project_id: &str, source: &str, message: &str) -> LogEntry {
        self.emit(project_id, "debug", source, message, None)
    }

    pub fn get_logs(
        &self,
        project_id: &str,
        since_id: Option<u64>,
        level_filter: Option<&str>,
        limit: Option<usize>,
    ) -> Vec<LogEntry> {
        if self.master_db_path.lock().unwrap().is_some() {
            return self.get_logs_from_db(project_id, since_id, level_filter, limit);
        }
        let limit = limit.unwrap_or(200);
        let buffer = self.buffer.read().unwrap();
        buffer
            .iter()
            .filter(|e| e.project_id == project_id)
            .filter(|e| if let Some(since) = since_id { e.id > since } else { true })
            .filter(|e| if let Some(lvl) = level_filter { e.level == lvl } else { true })
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    fn get_logs_from_db(
        &self,
        project_id: &str,
        since_id: Option<u64>,
        level_filter: Option<&str>,
        limit: Option<usize>,
    ) -> Vec<LogEntry> {
        let limit = limit.unwrap_or(200);
        let db_path = match self.master_db_path.lock().unwrap().clone() {
            Some(ref p) => p.clone(),
            None => return vec![],
        };
        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let mut stmt = match conn.prepare(
            "SELECT id, project_id, timestamp, level, source, message, details FROM logs WHERE project_id = ?1 ORDER BY id DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = match stmt.query_map(params![project_id], |row| {
            Ok(LogEntry {
                id: row.get::<_, i64>(0)? as u64,
                project_id: row.get(1)?,
                timestamp: row.get(2)?,
                level: row.get(3)?,
                source: row.get(4)?,
                message: row.get(5)?,
                details: row.get(6)?,
            })
        }) {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let mut all: Vec<LogEntry> = rows.filter_map(|r| r.ok()).collect();
        all.reverse();
        all.into_iter()
            .filter(|e| if let Some(since) = since_id { e.id > since } else { true })
            .filter(|e| if let Some(lvl) = level_filter { e.level == lvl } else { true })
            .take(limit)
            .collect()
    }

    pub fn clear(&self, project_id: &str) {
        // The in-memory buffer is shared across projects; we can only drop
        // entries belonging to this project.
        let mut buffer = self.buffer.write().unwrap();
        let before: VecDeque<LogEntry> =
            buffer.iter().filter(|e| e.project_id != project_id).cloned().collect();
        *buffer = before;
        if let Some(ref db_path) = self.master_db_path.lock().unwrap().clone() {
            if let Ok(conn) = rusqlite::Connection::open(db_path) {
                let _ = conn.execute(
                    "DELETE FROM logs WHERE project_id = ?1",
                    params![project_id],
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emit_and_retrieve() {
        let lm = LogManager::new();
        lm.emit("proj-1", "info", "test", "hello world", None);
        lm.emit("proj-1", "warn", "test", "warning msg", None);
        let logs = lm.get_logs("proj-1", None, None, Some(100));
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].level, "info");
        assert_eq!(logs[0].message, "hello world");
        assert_eq!(logs[1].level, "warn");
    }

    #[test]
    fn test_emit_different_projects() {
        let lm = LogManager::new();
        lm.emit("proj-a", "info", "src", "msg a", None);
        lm.emit("proj-b", "error", "src", "msg b", None);
        assert_eq!(lm.get_logs("proj-a", None, None, None).len(), 1);
        assert_eq!(lm.get_logs("proj-b", None, None, None).len(), 1);
        assert_eq!(lm.get_logs("proj-c", None, None, None).len(), 0);
    }

    #[test]
    fn test_level_filter() {
        let lm = LogManager::new();
        lm.emit("p", "info", "s", "info msg", None);
        lm.emit("p", "error", "s", "error msg", None);
        lm.emit("p", "warn", "s", "warn msg", None);
        let errors = lm.get_logs("p", None, Some("error"), None);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].level, "error");
        let infos = lm.get_logs("p", None, Some("info"), None);
        assert_eq!(infos.len(), 1);
    }

    #[test]
    fn test_since_id() {
        let lm = LogManager::new();
        lm.emit("p", "info", "s", "first", None);
        lm.emit("p", "info", "s", "second", None);
        lm.emit("p", "info", "s", "third", None);
        let logs = lm.get_logs("p", Some(1), None, None);
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].message, "second");
        assert_eq!(logs[1].message, "third");
    }

    #[test]
    fn test_limit() {
        let lm = LogManager::new();
        for i in 0..10 {
            lm.emit("p", "info", "s", &format!("msg {}", i), None);
        }
        let logs = lm.get_logs("p", None, None, Some(3));
        assert!(logs.len() <= 3);
    }

    #[test]
    fn test_clear() {
        let lm = LogManager::new();
        lm.emit("p", "info", "s", "msg", None);
        assert_eq!(lm.get_logs("p", None, None, None).len(), 1);
        lm.clear("p");
        assert_eq!(lm.get_logs("p", None, None, None).len(), 0);
    }

    #[test]
    fn test_ring_buffer_capacity() {
        let lm = LogManager::new();
        let n = MAX_LOG_ENTRIES + 100;
        for i in 0..n {
            lm.emit("p", "info", "s", &format!("msg {}", i), None);
        }
        let logs = lm.get_logs("p", None, None, Some(MAX_LOG_ENTRIES + 10));
        assert_eq!(logs.len(), MAX_LOG_ENTRIES);
        assert_eq!(logs[0].message, format!("msg {}", 100));
    }
}
