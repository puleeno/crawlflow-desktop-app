use crate::logs::{LogManager, ServiceStatusPayload};
use crate::pipeline::PipelineConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

fn project_db_path(project_id: &str) -> PathBuf {
    let data_dir = dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("crawlflow")
        .join("projects");
    let path = data_dir.join(format!("project_{}.db", project_id));
    // Ensure the directory exists
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        log::error!("Failed to create projects directory: {}", e);
    }
    path
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Idle,
    Running,
    Paused,
    Error(String),
    Stopped,
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
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        1970 + days / 365, 1, 1, h, m, s, ms)
}

fn status_str(s: &ServiceStatus) -> String {
    match s {
        ServiceStatus::Idle => "idle".into(),
        ServiceStatus::Running => "running".into(),
        ServiceStatus::Paused => "paused".into(),
        ServiceStatus::Error(e) => format!("error: {}", e),
        ServiceStatus::Stopped => "stopped".into(),
    }
}

struct ServiceState {
    status: Arc<RwLock<ServiceStatus>>,
    cancel_flag: Arc<AtomicBool>,
    pause_flag: Arc<AtomicBool>,
    started_at: Arc<RwLock<String>>,
    cycle_count: Arc<RwLock<u64>>,
    last_run_at: Arc<RwLock<String>>,
    last_error: Arc<RwLock<Option<String>>>,
    interval_seconds: Arc<RwLock<u64>>,
}

pub struct ServiceManager {
    services: Arc<RwLock<HashMap<String, ServiceState>>>,
    app_handle: RwLock<Option<AppHandle>>,
    log_manager: RwLock<Option<Arc<LogManager>>>,
}

impl ServiceManager {
    pub fn new_uninitialized() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            app_handle: RwLock::new(None),
            log_manager: RwLock::new(None),
        }
    }

    pub fn initialize(&self, app_handle: AppHandle, log_manager: Arc<LogManager>) {
        *self.app_handle.write().unwrap() = Some(app_handle);
        *self.log_manager.write().unwrap() = Some(log_manager);
    }

    fn app(&self) -> AppHandle {
        self.app_handle.read().unwrap().clone().expect("ServiceManager not initialized")
    }

    fn lm(&self) -> Arc<LogManager> {
        self.log_manager.read().unwrap().clone().expect("ServiceManager not initialized")
    }

    fn emit_st(
        app: &AppHandle,
        pid: &str,
        st: &str,
        cc: u64,
        sa: &str,
        lr: &str,
        le: &Option<String>,
    ) {
        let event = format!("service-status:{}", pid);
        let _ = app.emit(&event, ServiceStatusPayload {
            project_id: pid.to_string(),
            status: st.to_string(),
            cycle_count: cc,
            started_at: sa.to_string(),
            last_run_at: lr.to_string(),
            last_error: le.clone(),
        });
    }

    fn emit_for_state(&self, pid: &str, st: &ServiceState, status_str: &str) {
        let app = self.app();
        Self::emit_st(
            &app, pid, status_str,
            *st.cycle_count.read().unwrap(),
            &st.started_at.read().unwrap().clone(),
            &st.last_run_at.read().unwrap().clone(),
            &st.last_error.read().unwrap().clone(),
        );
    }

    pub fn start_service(
        &self,
        project_id: &str,
        nodes: Vec<serde_json::Value>,
        edges: Vec<serde_json::Value>,
        settings: serde_json::Value,
    ) -> Result<(), String> {
        let mut services = self.services.write().map_err(|e| e.to_string())?;
        if services.contains_key(project_id) {
            return Err("Service already exists for this project".into());
        }

        let interval = settings.get("intervalSeconds")
            .and_then(|v| v.as_u64()).unwrap_or(60);

        let status = Arc::new(RwLock::new(ServiceStatus::Running));
        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let started_at = Arc::new(RwLock::new(now_str()));
        let cycle_count = Arc::new(RwLock::new(0u64));
        let last_run_at = Arc::new(RwLock::new(String::new()));
        let last_error = Arc::new(RwLock::new(None));
        let interval_seconds = Arc::new(RwLock::new(interval));

        let state = ServiceState {
            status: status.clone(),
            cancel_flag: cancel.clone(),
            pause_flag: pause.clone(),
            started_at: started_at.clone(),
            cycle_count: cycle_count.clone(),
            last_run_at: last_run_at.clone(),
            last_error: last_error.clone(),
            interval_seconds: interval_seconds.clone(),
        };

        let pid = project_id.to_string();
        let app = self.app();
        let lm = self.lm();

        let nodes_count = nodes.len();
        let edges_count = edges.len();
        let deser_nodes: Vec<crate::pipeline::PipelineNode> = nodes
            .into_iter()
            .filter_map(|v| {
                let result: Option<crate::pipeline::PipelineNode> = serde_json::from_value(v.clone()).ok();
                if result.is_none() {
                    log::warn!("Failed to deserialize node: {:?}", v);
                }
                result
            })
            .collect();
        let deser_edges: Vec<crate::pipeline::PipelineEdge> = edges
            .into_iter()
            .filter_map(|v| {
                let result: Option<crate::pipeline::PipelineEdge> = serde_json::from_value(v.clone()).ok();
                if result.is_none() {
                    log::warn!("Failed to deserialize edge: {:?}", v);
                }
                result
            })
            .collect();
        log::info!("start_service: received {} nodes, deserialized {} nodes; received {} edges, deserialized {} edges",
            nodes_count, deser_nodes.len(), edges_count, deser_edges.len());

        let pcfg = PipelineConfig {
            nodes: deser_nodes,
            edges: deser_edges,
            settings,
        };

        tauri::async_runtime::spawn(async move {
            Self::run_loop(
                &pid, pcfg, &app, &lm,
                cancel, pause, status,
                cycle_count, last_run_at, last_error,
                interval_seconds, started_at,
            ).await;
        });

        services.insert(project_id.to_string(), state);

        // Log and emit after inserting
        drop(services);
        let _ = self.lm().info(project_id, "system", "Service started");
        let services_r = self.services.read().unwrap();
        if let Some(st) = services_r.get(project_id) {
            self.emit_for_state(project_id, st, "running");
        }

        Ok(())
    }

    async fn run_loop(
        project_id: &str,
        config: PipelineConfig,
        app: &AppHandle,
        lm: &Arc<LogManager>,
        cancel: Arc<AtomicBool>,
        pause: Arc<AtomicBool>,
        status: Arc<RwLock<ServiceStatus>>,
        cycle_count: Arc<RwLock<u64>>,
        last_run_at: Arc<RwLock<String>>,
        last_error: Arc<RwLock<Option<String>>>,
        interval_seconds: Arc<RwLock<u64>>,
        started_at: Arc<RwLock<String>>,
    ) {
        lm.info(project_id, "system", "Service loop entered");

        while !cancel.load(Ordering::Relaxed) {
            while pause.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
            if cancel.load(Ordering::Relaxed) { break; }

            let _ = {
                let mut cc = cycle_count.write().unwrap();
                *cc += 1;
                *cc
            };

            let cc = *cycle_count.read().unwrap();
            *last_run_at.write().unwrap() = now_str();
            *status.write().unwrap() = ServiceStatus::Running;

            lm.info(project_id, "system", &format!("--- Cycle #{} ---", cc));
            Self::emit_st(app, project_id, "running", cc,
                &started_at.read().unwrap(), &last_run_at.read().unwrap(), &last_error.read().unwrap());

            let db_path = project_db_path(project_id);
            let result = crate::pipeline::execute_repository_pipeline(
                &config,
                &db_path,
                lm,
                project_id,
                None,
            );

            if result.success {
                lm.info(project_id, "system",
                    &format!("Cycle #{} complete: ingested {}, matched {}, processed {}, failed {}",
                        cc, result.ingested, result.matched, result.processed, result.failed));
                *last_error.write().unwrap() = None;
            } else {
                let err = result.error.unwrap_or_else(|| "Unknown error".into());
                lm.error(project_id, "system", &format!("Cycle #{} failed (phase: {}): {}",
                    cc, result.phase, err));
                *last_error.write().unwrap() = Some(err.clone());
                *status.write().unwrap() = ServiceStatus::Error(err.clone());
            }

            Self::emit_st(app, project_id,
                &status_str(&*status.read().unwrap()), cc,
                &started_at.read().unwrap(), &last_run_at.read().unwrap(), &last_error.read().unwrap());

            let interval = *interval_seconds.read().unwrap();
            for _ in 0..interval {
                if cancel.load(Ordering::Relaxed) { break; }
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }

        lm.info(project_id, "system", "Service stopped");
        *status.write().unwrap() = ServiceStatus::Stopped;
        let cc = *cycle_count.read().unwrap();
        Self::emit_st(app, project_id, "stopped", cc,
            &started_at.read().unwrap(), &last_run_at.read().unwrap(), &last_error.read().unwrap());
    }

    pub fn stop_service(&self, project_id: &str) -> Result<(), String> {
        let mut services = self.services.write().map_err(|e| e.to_string())?;
        if let Some(st) = services.remove(project_id) {
            st.cancel_flag.store(true, Ordering::Relaxed);
            let _ = self.lm().info(project_id, "system", "Service stopping");
            self.emit_for_state(project_id, &st, "stopped");
            Ok(())
        } else {
            Err("Service not found".into())
        }
    }

    pub fn pause_service(&self, project_id: &str) -> Result<(), String> {
        let services = self.services.read().map_err(|e| e.to_string())?;
        if let Some(st) = services.get(project_id) {
            st.pause_flag.store(true, Ordering::Relaxed);
            *st.status.write().unwrap() = ServiceStatus::Paused;
            let _ = self.lm().info(project_id, "system", "Service paused");
            self.emit_for_state(project_id, st, "paused");
            Ok(())
        } else {
            Err("Service not found".into())
        }
    }

    pub fn resume_service(&self, project_id: &str) -> Result<(), String> {
        let services = self.services.read().map_err(|e| e.to_string())?;
        if let Some(st) = services.get(project_id) {
            st.pause_flag.store(false, Ordering::Relaxed);
            *st.status.write().unwrap() = ServiceStatus::Running;
            let _ = self.lm().info(project_id, "system", "Service resumed");
            self.emit_for_state(project_id, st, "running");
            Ok(())
        } else {
            Err("Service not found".into())
        }
    }

    pub fn get_service_info(&self, project_id: &str) -> Option<ServiceInfo> {
        let services = self.services.read().ok()?;
        let st = services.get(project_id)?;
        let info = ServiceInfo {
            project_id: project_id.to_string(),
            status: status_str(&*st.status.read().unwrap()),
            cycle_count: *st.cycle_count.read().unwrap(),
            started_at: st.started_at.read().unwrap().clone(),
            last_run_at: st.last_run_at.read().unwrap().clone(),
            last_error: st.last_error.read().unwrap().clone(),
            interval_seconds: *st.interval_seconds.read().unwrap(),
        };
        Some(info)
    }

    pub fn list_service_infos(&self) -> Vec<ServiceInfo> {
        let services = self.services.read().unwrap();
        services.iter().map(|(pid, st)| {
            let status = status_str(&*st.status.read().unwrap());
            let cc = *st.cycle_count.read().unwrap();
            let sa = st.started_at.read().unwrap().clone();
            let lr = st.last_run_at.read().unwrap().clone();
            let le = st.last_error.read().unwrap().clone();
            let iv = *st.interval_seconds.read().unwrap();
            ServiceInfo {
                project_id: pid.clone(),
                status,
                cycle_count: cc,
                started_at: sa,
                last_run_at: lr,
                last_error: le,
                interval_seconds: iv,
            }
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_str() {
        assert_eq!(status_str(&ServiceStatus::Idle), "idle");
        assert_eq!(status_str(&ServiceStatus::Running), "running");
        assert_eq!(status_str(&ServiceStatus::Paused), "paused");
        assert_eq!(status_str(&ServiceStatus::Stopped), "stopped");
        assert!(status_str(&ServiceStatus::Error("boom".into())).starts_with("error:"));
    }

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
        };
        assert_eq!(info.status, "error: timeout");
        assert_eq!(info.last_error.unwrap(), "timeout");
    }

    #[test]
    fn test_now_str_format() {
        let s = now_str();
        assert!(s.contains("T"));
        assert!(s.ends_with("Z"));
        assert_eq!(s.len(), 24);
    }

    #[test]
    fn test_service_status_serde_roundtrip() {
        for (variant, expected) in &[
            (ServiceStatus::Idle, "\"idle\""),
            (ServiceStatus::Running, "\"running\""),
            (ServiceStatus::Paused, "\"paused\""),
            (ServiceStatus::Stopped, "\"stopped\""),
        ] {
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(json, *expected);
            let back: ServiceStatus = serde_json::from_str(expected).unwrap();
            assert_eq!(format!("{:?}", back), format!("{:?}", variant));
        }
    }

    #[test]
    fn test_service_status_error_serde() {
        let err = ServiceStatus::Error("boom".into());
        let json = serde_json::to_string(&err).unwrap();
        assert_eq!(json, r#"{"error":"boom"}"#);
        let back: ServiceStatus = serde_json::from_str(r#"{"error":"boom"}"#).unwrap();
        match back {
            ServiceStatus::Error(msg) => assert_eq!(msg, "boom"),
            _ => panic!("Expected Error variant"),
        }
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
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ServiceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, "error: timeout");
        assert_eq!(back.last_error, Some("timeout".into()));
        assert_eq!(back.interval_seconds, 30);
    }
}
