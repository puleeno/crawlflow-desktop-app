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
    // Bridge to pass to pipeline::execute_pipeline which expects LogManager
    fn as_log_manager(db_path: PathBuf) -> crawlflow_lib::logs::LogManager {
        let lm = crawlflow_lib::logs::LogManager::new();
        lm.set_master_db_path(db_path);
        lm
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
        .join("com.CrawlFlow.desktop")
}

fn master_db_path() -> PathBuf {
    get_app_data_dir().join("crawlflow.db")
}

fn project_db_path(db_filename: &str) -> PathBuf {
    get_app_data_dir().join(db_filename)
}

fn get_user_plugins_dir() -> PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.CrawlFlow.desktop")
        .join("plugins")
}

fn get_builtin_plugins_dir() -> Option<PathBuf> {
    let bundled_dir = std::env::current_exe().ok().and_then(|path| {
        path.parent()?
            .parent()
            .map(|resources| resources.join("plugins"))
    });
    if bundled_dir.as_ref().is_some_and(|path| path.is_dir()) {
        return bundled_dir;
    }

    let dev_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|path| path.join("plugins"));
    dev_dir.filter(|path| path.is_dir())
}

fn get_enabled_python_plugin_ids() -> Result<std::collections::HashSet<String>, String> {
    let connection = open_db(&master_db_path())?;
    let mut statement = connection
        .prepare("SELECT id FROM extensions WHERE type = 'plugin' AND enabled = 1")
        .map_err(|error| format!("Cannot read enabled plugins: {}", error))?;
    let plugin_ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("Cannot query enabled plugins: {}", error))?
        .filter_map(Result::ok)
        .map(|plugin_id| plugin_id.strip_prefix("py-").unwrap_or(&plugin_id).to_string())
        .collect();
    Ok(plugin_ids)
}

fn create_python_plugin_engine(
    enabled_plugin_ids: &std::collections::HashSet<String>,
) -> Result<crawlflow_lib::python_plugins::PythonPluginEngine, String> {
    let user_dir = get_user_plugins_dir();
    std::fs::create_dir_all(&user_dir).map_err(|error| {
        format!(
            "Cannot create user plugin directory {:?}: {}",
            user_dir, error
        )
    })?;

    let mut engine =
        crawlflow_lib::python_plugins::PythonPluginEngine::new(get_builtin_plugins_dir(), user_dir);
    let discovered = engine.discover()?;

    // If the extensions table is empty on first run, keep all discovered plugins available.
    // This allows project-specific plugin sources such as oreka-shop-crawler to run even
    // before the user manually toggles the plugin in the UI.
    if !enabled_plugin_ids.is_empty() {
        engine.retain_plugins(enabled_plugin_ids);
    }

    let enabled_discovered: Vec<_> = discovered
        .into_iter()
        .filter(|plugin_id| enabled_plugin_ids.is_empty() || enabled_plugin_ids.contains(plugin_id))
        .collect();
    println!("[SERVICE] Python plugins initialized: {:?}", enabled_discovered);
    Ok(engine)
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

fn ensure_runtime_table(conn: &rusqlite::Connection) {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS project_runtime (
            project_id TEXT PRIMARY KEY,
            runner_status TEXT NOT NULL DEFAULT 'stopped',
            runner_pid INTEGER,
            runner_type TEXT DEFAULT 'service',
            edit_pid INTEGER,
            cycle_count INTEGER NOT NULL DEFAULT 0,
            last_run_at TEXT,
            last_error TEXT,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
        [],
    )
    .ok();
}

fn is_pid_alive(pid: u32) -> bool {
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

/// Returns true if the project has an active editor (desktop app with a live PID)
fn is_project_being_edited(conn: &rusqlite::Connection, project_id: &str) -> bool {
    let result: rusqlite::Result<Option<i64>> = conn.query_row(
        "SELECT edit_pid FROM project_runtime WHERE project_id = ?1",
        rusqlite::params![project_id],
        |row| row.get(0),
    );
    match result {
        Ok(Some(pid)) if pid > 0 => is_pid_alive(pid as u32),
        _ => false,
    }
}

fn set_runner_status(
    conn: &rusqlite::Connection,
    project_id: &str,
    status: &str,
    pid: Option<i64>,
    cycle: Option<i64>,
    last_run: Option<&str>,
    last_error: Option<&str>,
) {
    conn.execute(
        "INSERT INTO project_runtime (project_id, runner_status, runner_pid, runner_type, cycle_count, last_run_at, last_error, updated_at)
         VALUES (?1, ?2, ?3, 'service', COALESCE(?4, 0), ?5, ?6, datetime('now'))
         ON CONFLICT(project_id) DO UPDATE SET
             runner_status = ?2,
             runner_pid = ?3,
             runner_type = 'service',
             cycle_count = COALESCE(?4, cycle_count),
             last_run_at = COALESCE(?5, last_run_at),
             last_error = ?6,
             updated_at = datetime('now')",
        rusqlite::params![project_id, status, pid, cycle, last_run, last_error],
    ).ok();
}

// ── Per-project async loop ─────────────────────────────────────────────────────────

async fn run_project_loop(proj: ProjectRow, interval_secs: u64, shutdown: Arc<AtomicBool>) {
    let project_id = proj.id.clone();
    let db_path = project_db_path(&proj.db_path);
    let master_db = master_db_path();

    // Ensure project_runtime table exists
    if let Ok(conn) = rusqlite::Connection::open(&master_db) {
        ensure_runtime_table(&conn);
    }

    let lm = Arc::new(SimpleLogger::as_log_manager(master_db_path()));
    let enabled_plugin_ids = match get_enabled_python_plugin_ids() {
        Ok(plugin_ids) => plugin_ids,
        Err(error) => {
            lm.error(
                &project_id,
                "service",
                &format!("Could not read enabled plugin settings: {}", error),
            );
            return;
        }
    };
    let mut python_engine = match create_python_plugin_engine(&enabled_plugin_ids) {
        Ok(engine) => engine,
        Err(error) => {
            lm.error(
                &project_id,
                "service",
                &format!("Python plugin initialization failed: {}", error),
            );
            return;
        }
    };
    let self_pid = std::process::id() as i64;
    let mut cycle = 0u64;

    // Cancellation token for stopping pipeline mid-execution
    let cancellation = Arc::new(AtomicBool::new(false));
    let cancellation_clone = cancellation.clone();
    let project_id_clone = project_id.clone();
    let master_db_clone = master_db.clone();

    // Spawn background task to monitor service_control and update cancellation token
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
        loop {
            interval.tick().await;
            if let Ok(conn) = rusqlite::Connection::open(&master_db_clone) {
                let control: Result<String, _> = conn.query_row(
                    "SELECT service_control FROM project_runtime WHERE project_id = ?1",
                    rusqlite::params![&project_id_clone],
                    |row| row.get(0),
                );
                if let Ok(control_str) = control {
                    if control_str == "stop" {
                        cancellation_clone.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }
    });

    lm.info(
        &project_id,
        "service",
        &format!(
            "Loop started for '{}' (every {}s)",
            proj.name, interval_secs
        ),
    );

    while !shutdown.load(Ordering::Relaxed) {
        cycle += 1;
        lm.info(&project_id, "service", &format!("--- Cycle #{} ---", cycle));

        // Check if desktop app has this project open for editing
        let conn_result = rusqlite::Connection::open(&master_db);
        let is_editing = conn_result
            .as_ref()
            .map(|conn| is_project_being_edited(conn, &project_id))
            .unwrap_or(false);

        // Check if user requested a stop/pause via the desktop app UI
        let service_control = conn_result
            .as_ref()
            .map(|conn| {
                conn.query_row(
                    "SELECT service_control FROM project_runtime WHERE project_id = ?1",
                    rusqlite::params![&project_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap_or_else(|_| "run".to_string())
            })
            .unwrap_or_else(|_| "run".to_string());

        if is_editing || service_control == "paused" || service_control == "stop" {
            let reason = if is_editing {
                "open in desktop app"
            } else if service_control == "paused" {
                "paused by user"
            } else {
                "stopped by user"
            };
            lm.info(
                &project_id,
                "service",
                &format!("Project '{}' is {}. Skipping cycle.", proj.name, reason),
            );
            // If stopped, exit the loop entirely
            if service_control == "stop" {
                break;
            }
            for _ in 0..interval_secs {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
            continue;
        }

        // Mark as running in shared SQLite and reset service_control
        if let Ok(conn) = rusqlite::Connection::open(&master_db) {
            set_runner_status(
                &conn,
                &project_id,
                "running",
                Some(self_pid),
                Some(cycle as i64),
                None,
                None,
            );
            // Reset service_control to "run" so it continues unless explicitly stopped
            let _ = conn.execute(
                "UPDATE project_runtime SET service_control = 'run' WHERE project_id = ?1",
                rusqlite::params![&project_id],
            );
        }

        match load_pipeline(&db_path) {
            Ok((nodes, edges)) => {
                let config = crawlflow_lib::pipeline::PipelineConfig {
                    nodes,
                    edges,
                    settings: serde_json::json!({}),
                };
                // Reset cancellation before execution
                cancellation.store(false, Ordering::Relaxed);
                let result = crawlflow_lib::pipeline::execute_repository_pipeline(
                    &config,
                    &db_path,
                    &lm,
                    &project_id,
                    Some(&mut python_engine),
                    Some(&cancellation),
                )
                .await;
                let now = {
                    let secs = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let (h, m, s) = ((secs % 86400) / 3600, (secs % 3600) / 60, secs % 60);
                    format!(
                        "{:04}-01-01T{:02}:{:02}:{:02}Z",
                        1970 + secs / 31536000,
                        h,
                        m,
                        s
                    )
                };
                if let Ok(conn) = rusqlite::Connection::open(&master_db) {
                    if result.success {
                        set_runner_status(
                            &conn,
                            &project_id,
                            "idle",
                            Some(self_pid),
                            Some(cycle as i64),
                            Some(&now),
                            None,
                        );
                        lm.info(
                            &project_id,
                            "service",
                            &format!(
                                "Cycle #{}: ingested={}, matched={}, processed={}, failed={}",
                                cycle,
                                result.ingested,
                                result.matched,
                                result.processed,
                                result.failed
                            ),
                        );
                    } else {
                        let err = result.error.clone().unwrap_or_default();
                        set_runner_status(
                            &conn,
                            &project_id,
                            "idle",
                            Some(self_pid),
                            Some(cycle as i64),
                            Some(&now),
                            Some(&err),
                        );
                        lm.error(
                            &project_id,
                            "service",
                            &format!("Cycle #{} failed ({}): {}", cycle, result.phase, err),
                        );
                    }
                }
            }
            Err(e) => {
                if let Ok(conn) = rusqlite::Connection::open(&master_db) {
                    set_runner_status(
                        &conn,
                        &project_id,
                        "idle",
                        Some(self_pid),
                        None,
                        None,
                        Some(&e),
                    );
                }
                lm.error(
                    &project_id,
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

    // Mark stopped on exit
    if let Ok(conn) = rusqlite::Connection::open(&master_db) {
        set_runner_status(&conn, &project_id, "stopped", None, None, None, None);
    }
    lm.info(&project_id, "service", "Loop stopped.");
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
    crawlflow_lib::request_clients::shutdown_global_chrome();
    println!("[SERVICE] All stopped. Goodbye.");
}
