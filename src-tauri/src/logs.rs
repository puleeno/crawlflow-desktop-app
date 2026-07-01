use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Emitter};
use std::time::{SystemTime, UNIX_EPOCH};

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

pub struct LogManager {
    buffers: Arc<RwLock<HashMap<String, VecDeque<LogEntry>>>>,
    next_id: Arc<RwLock<u64>>,
    app_handle: Mutex<Option<AppHandle>>,
}

impl LogManager {
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
            app_handle: Mutex::new(None),
        }
    }

    /// Takes &self because it uses interior mutability (Mutex)
    pub fn set_app_handle(&self, handle: AppHandle) {
        *self.app_handle.lock().unwrap() = Some(handle);
    }

    pub fn app_handle(&self) -> Option<AppHandle> {
        self.app_handle.lock().unwrap().clone()
    }

    fn now_iso() -> String {
        let d = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = d.as_secs();
        let millis = d.subsec_millis();
        let days = secs / 86400;
        let time_secs = secs % 86400;
        let hours = time_secs / 3600;
        let mins = (time_secs % 3600) / 60;
        let sec = time_secs % 60;
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", 1970 + days / 365, 1, 1, hours, mins, sec, millis)
    }

    fn now_epoch() -> String {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string()
    }

    pub fn emit(
        &self,
        project_id: &str,
        level: &str,
        source: &str,
        message: &str,
        details: Option<String>,
    ) -> LogEntry {
        let id = {
            let mut next = self.next_id.write().unwrap();
            let id = *next;
            *next += 1;
            id
        };

        let entry = LogEntry {
            id,
            project_id: project_id.to_string(),
            timestamp: Self::now_iso(),
            level: level.to_string(),
            source: source.to_string(),
            message: message.to_string(),
            details,
        };

        {
            let mut buffers = self.buffers.write().unwrap();
            let buffer = buffers
                .entry(project_id.to_string())
                .or_insert_with(|| VecDeque::with_capacity(MAX_LOG_ENTRIES));
            if buffer.len() >= MAX_LOG_ENTRIES {
                buffer.pop_front();
            }
            buffer.push_back(entry.clone());
        }

        if let Some(ref handle) = *self.app_handle.lock().unwrap() {
            let event_name = format!("project-log:{}", project_id);
            let _ = handle.emit(&event_name, &entry);
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
        let limit = limit.unwrap_or(200);
        let buffers = self.buffers.read().unwrap();
        if let Some(buffer) = buffers.get(project_id) {
            buffer
                .iter()
                .filter(|e| {
                    if let Some(since) = since_id {
                        e.id > since
                    } else {
                        true
                    }
                })
                .filter(|e| {
                    if let Some(lvl) = level_filter {
                        e.level == lvl
                    } else {
                        true
                    }
                })
                .rev()
                .take(limit)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        } else {
            vec![]
        }
    }

    pub fn clear(&self, project_id: &str) {
        let mut buffers = self.buffers.write().unwrap();
        if let Some(buffer) = buffers.get_mut(project_id) {
            buffer.clear();
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
    fn test_convenience_methods() {
        let lm = LogManager::new();
        lm.info("p", "src", "info msg");
        lm.warn("p", "src", "warn msg");
        lm.error("p", "src", "error msg");
        lm.debug("p", "src", "debug msg");
        let logs = lm.get_logs("p", None, None, None);
        assert_eq!(logs.len(), 4);
        assert_eq!(logs[0].level, "info");
        assert_eq!(logs[1].level, "warn");
        assert_eq!(logs[2].level, "error");
        assert_eq!(logs[3].level, "debug");
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
