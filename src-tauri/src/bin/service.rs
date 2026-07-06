//! CrawlFlow background service — headless, no GUI, no Tauri runtime, no dock icon.
//!
//! Usage:
//!   crawlflow-service --project <PROJECT_ID> [--interval <SECONDS>]
//!   crawlflow-service --all [--interval <SECONDS>]
//!
//! Runs as a plain background process. On macOS there is no Dock icon.
//! On Windows, built with windows_subsystem = "windows" so no console window appears.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ── Minimal inline log manager (avoids importing Tauri-coupled crawlflow_lib::logs) ──

struct SimpleLogger;

impl SimpleLogger {
    fn log_msg(project_id: &str, level: &str, source: &str, message: &str) {
        let now = {
            let d = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            let s = d.as_secs();
            let ms = d.subsec_millis();
            format!(
                "{:02}:{:02}:{:02}.{:03}",
                (s % 86400) / 3600,
                (s % 3600) / 60,
                s % 60,
                ms
            )
        };
        println!(
            "[{}] [{}] [{}] [{}] {}",
            now,
            level.to_uppercase(),
            project_id,
            source,
            message
        );
    }

    // Bridge to pass to pipeline::execute_pipeline which expects LogManager
    fn as_log_manager() -> crawlflow_lib::logs::LogManager {
        crawlflow_lib::logs::LogManager::new() // no AppHandle → no Tauri emit, just in-memory buffer
    }
}

// ── SQLite access (rusqlite, no async needed) ──────────────────────────────────────

#[derive(Debug)]
struct ProjectRow {
    id: String,
    name: String,
    db_path: String,
}

fn get_app_data_dir() -> PathBuf {
    // Mirrors Tauri v2 data directory logic
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.crawlflow.desktop")
}

fn master_db_path() -> PathBuf {
    get_app_data_dir().join("crawlflow.db")
}

fn project_db_path(db_filename: &str) -> PathBuf {
    get_app_data_dir().join(db_filename)
}

fn open_db(path: &PathBuf) -> Result<rusqlite::Connection, String> {
    rusqlite::Connection::open(path).map_err(|e| format!("Cannot open {:?}: {}", path, e))
}

fn list_enabled_projects() -> Result<Vec<ProjectRow>, String> {
    let path = master_db_path();
    let conn = open_db(&path)?;
    let mut stmt = conn
        .prepare("SELECT id, name, db_path FROM projects WHERE status = 'enabled' ORDER BY updated_at DESC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(ProjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                db_path: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

fn load_pipeline(
    project_db: &PathBuf,
) -> Result<
    (
        Vec<crawlflow_lib::pipeline::PipelineNode>,
        Vec<crawlflow_lib::pipeline::PipelineEdge>,
    ),
    String,
> {
    let conn = open_db(project_db)?;

    let mut ns = conn
        .prepare("SELECT id, type, label, position_x, position_y, data FROM nodes")
        .map_err(|e| e.to_string())?;
    let nodes: Vec<crawlflow_lib::pipeline::PipelineNode> = ns
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .map(|(id, node_type, label, px, py, data_str)| {
            let data = serde_json::from_str(&data_str).unwrap_or(serde_json::Value::Null);
            crawlflow_lib::pipeline::PipelineNode {
                id,
                node_type,
                label,
                data,
                position: Some(serde_json::json!({"x": px, "y": py})),
            }
        })
        .collect();

    let mut es = conn
        .prepare("SELECT id, source, target, source_handle, target_handle FROM edges")
        .map_err(|e| e.to_string())?;
    let edges: Vec<crawlflow_lib::pipeline::PipelineEdge> = es
        .query_map([], |row| {
            Ok(crawlflow_lib::pipeline::PipelineEdge {
                id: row.get(0)?,
                source: row.get(1)?,
                target: row.get(2)?,
                source_handle: row.get(3)?,
                target_handle: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok((nodes, edges))
}

// ── Per-project async loop ─────────────────────────────────────────────────────────

async fn run_project_loop(proj: ProjectRow, interval_secs: u64, shutdown: Arc<AtomicBool>) {
    let pid = proj.id.clone();
    let db_path = project_db_path(&proj.db_path);
    SimpleLogger::log_msg(
        &pid,
        "info",
        "service",
        &format!(
            "Loop started for '{}' (every {}s)",
            proj.name, interval_secs
        ),
    );

    let lm = Arc::new(SimpleLogger::as_log_manager());
    let mut cycle = 0u64;

    while !shutdown.load(Ordering::Relaxed) {
        cycle += 1;
        SimpleLogger::log_msg(
            &pid,
            "info",
            "service",
            &format!("--- Cycle #{} ---", cycle),
        );

        match load_pipeline(&db_path) {
            Ok((nodes, edges)) => {
                let config = crawlflow_lib::pipeline::PipelineConfig {
                    nodes,
                    edges,
                    settings: serde_json::json!({}),
                };
                let result = crawlflow_lib::pipeline::execute_pipeline(&config, &lm, &pid);
                if result.success {
                    SimpleLogger::log_msg(
                        &pid,
                        "info",
                        "service",
                        &format!(
                            "Cycle #{}: {} steps, {} items",
                            cycle,
                            result.steps.len(),
                            result.final_output.len()
                        ),
                    );
                } else {
                    SimpleLogger::log_msg(
                        &pid,
                        "error",
                        "service",
                        &format!(
                            "Cycle #{} failed: {}",
                            cycle,
                            result.error.unwrap_or_default()
                        ),
                    );
                }
            }
            Err(e) => {
                SimpleLogger::log_msg(
                    &pid,
                    "error",
                    "service",
                    &format!("Load pipeline failed: {}", e),
                );
            }
        }

        // Sleep in 1s ticks for responsive shutdown
        for _ in 0..interval_secs {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    SimpleLogger::log_msg(&pid, "info", "service", "Loop stopped.");
}

// ── Entry point ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let mut target_project: Option<String> = None;
    let mut run_all = false;
    let mut interval_secs: u64 = 60;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--project" => {
                i += 1;
                target_project = args.get(i).cloned();
            }
            "--all" => {
                run_all = true;
            }
            "--interval" => {
                i += 1;
                interval_secs = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(60);
            }
            _ => {}
        }
        i += 1;
    }

    if target_project.is_none() && !run_all {
        eprintln!("CrawlFlow Service");
        eprintln!("Usage:");
        eprintln!("  crawlflow-service --project <PROJECT_ID> [--interval <SECONDS>]");
        eprintln!("  crawlflow-service --all [--interval <SECONDS>]");
        std::process::exit(1);
    }

    println!("[SERVICE] CrawlFlow background service starting");
    println!("[SERVICE] Data dir : {:?}", get_app_data_dir());
    println!("[SERVICE] Master DB: {:?}", master_db_path());
    println!("[SERVICE] Interval : {}s", interval_secs);

    let shutdown = Arc::new(AtomicBool::new(false));
    {
        let flag = shutdown.clone();
        ctrlc::set_handler(move || {
            println!("\n[SERVICE] Shutdown signal — stopping...");
            flag.store(true, Ordering::Relaxed);
        })
        .ok();
    }

    // Resolve project list
    let all_projects = match list_enabled_projects() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[SERVICE] DB error: {}", e);
            std::process::exit(1);
        }
    };

    let projects: Vec<ProjectRow> = if run_all {
        println!("[SERVICE] Running {} enabled projects", all_projects.len());
        all_projects
    } else {
        let pid = target_project.as_deref().unwrap_or("");
        let found: Vec<_> = all_projects.into_iter().filter(|p| p.id == pid).collect();
        if found.is_empty() {
            eprintln!("[SERVICE] Project '{}' not found or not enabled", pid);
            std::process::exit(1);
        }
        found
    };

    if projects.is_empty() {
        println!("[SERVICE] No enabled projects. Exiting.");
        return;
    }

    let mut handles = Vec::new();
    for proj in projects {
        let sd = shutdown.clone();
        handles.push(tokio::spawn(run_project_loop(proj, interval_secs, sd)));
    }

    for h in handles {
        let _ = h.await;
    }
    println!("[SERVICE] All stopped. Goodbye.");
}
