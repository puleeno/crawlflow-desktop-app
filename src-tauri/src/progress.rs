use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressInfo {
    pub items_total: u64,
    pub items_processed: u64,
    pub items_success: u64,
    pub items_failed: u64,
    pub progress_pct: f64,
    pub avg_time_ms: f64,
    pub total_time_ms: u64,
    pub started_at: String,
    pub message: String,
}

impl Default for ProgressInfo {
    fn default() -> Self {
        Self {
            items_total: 0,
            items_processed: 0,
            items_success: 0,
            items_failed: 0,
            progress_pct: 0.0,
            avg_time_ms: 0.0,
            total_time_ms: 0,
            started_at: now_str(),
            message: String::new(),
        }
    }
}

fn now_str() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    let ms = d.subsec_millis();
    let days = secs / 86400;
    let t = secs % 86400;
    let h = t / 3600;
    let m = (t % 3600) / 60;
    let s = t % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        1970 + days / 365,
        1,
        1,
        h,
        m,
        s,
        ms
    )
}

static PROGRESS: LazyLock<Mutex<HashMap<String, ProgressInfo>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn update_progress(project_id: &str, info: ProgressInfo) {
    if let Ok(mut map) = PROGRESS.lock() {
        map.insert(project_id.to_string(), info);
    }
}

pub fn get_progress(project_id: &str) -> Option<ProgressInfo> {
    PROGRESS.lock().ok().and_then(|map| map.get(project_id).cloned())
}

#[allow(dead_code)]
pub fn init_progress(project_id: &str) {
    if let Ok(mut map) = PROGRESS.lock() {
        map.entry(project_id.to_string()).or_insert_with(|| {
            let mut p = ProgressInfo::default();
            p.started_at = now_str();
            p
        });
    }
}

#[allow(dead_code)]
pub fn remove_progress(project_id: &str) {
    if let Ok(mut map) = PROGRESS.lock() {
        map.remove(project_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_defaults() {
        let p = ProgressInfo::default();
        assert_eq!(p.items_total, 0);
        assert_eq!(p.items_processed, 0);
        assert_eq!(p.progress_pct, 0.0);
        assert!(!p.started_at.is_empty());
    }

    #[test]
    fn test_update_and_get_progress() {
        init_progress("test-proj-1");
        let p = get_progress("test-proj-1");
        assert!(p.is_some());
        assert_eq!(p.unwrap().items_total, 0);

        let info = ProgressInfo {
            items_total: 100,
            items_processed: 50,
            items_success: 48,
            items_failed: 2,
            progress_pct: 50.0,
            avg_time_ms: 1500.0,
            total_time_ms: 75000,
            ..Default::default()
        };
        update_progress("test-proj-1", info);

        let p2 = get_progress("test-proj-1").unwrap();
        assert_eq!(p2.items_total, 100);
        assert_eq!(p2.items_processed, 50);
        assert_eq!(p2.progress_pct, 50.0);
        assert_eq!(p2.avg_time_ms, 1500.0);

        remove_progress("test-proj-1");
        assert!(get_progress("test-proj-1").is_none());
    }

    #[test]
    fn test_progress_multiple_projects() {
        init_progress("proj-a");
        init_progress("proj-b");

        update_progress("proj-a", ProgressInfo {
            items_total: 10, ..Default::default()
        });
        update_progress("proj-b", ProgressInfo {
            items_total: 20, ..Default::default()
        });

        assert_eq!(get_progress("proj-a").unwrap().items_total, 10);
        assert_eq!(get_progress("proj-b").unwrap().items_total, 20);

        remove_progress("proj-a");
        remove_progress("proj-b");
    }

    #[test]
    fn test_get_nonexistent() {
        assert!(get_progress("non-existent-project").is_none());
    }
}
