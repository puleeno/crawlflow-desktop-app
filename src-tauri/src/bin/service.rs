//! CrawlFlow background service — headless, no GUI, no Tauri runtime, no dock icon.
//!
//! Usage:
//!   crawlflow-service --project <PROJECT_ID> [--interval <SECONDS>]
//!   crawlflow-service --all [--interval <SECONDS>]
//!   crawlflow-service --service --all               (Windows Service mode)
//!
//! Runs as a plain background process or as a proper Windows Service.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crawlflow_lib::services::{get_export_settings, read_project_refresh_strategy};
use crawlflow_lib::ws::{self, WsHub};

/// Current UTC timestamp as an ISO-8601-ish string for progress/status stamps.
fn now_iso() -> String {
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
}

// ── Minimal inline log manager (avoids importing Tauri-coupled crawlflow_lib::logs) ──

use std::io::Write;

struct SimpleLogger;

impl SimpleLogger {
    // Bridge to pass to pipeline::execute_pipeline which expects LogManager
    fn as_log_manager(db_path: PathBuf) -> crawlflow_lib::logs::LogManager {
        let lm = crawlflow_lib::logs::LogManager::new();
        lm.set_master_db_path(db_path);
        lm
    }
}

// ── File logger for Windows Service mode ─────────────────────────────────────────

static mut SERVICE_LOG_FILE: Option<std::fs::File> = None;

fn service_log_dir() -> PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.CrawlFlow.desktop")
        .join("logs")
}

fn init_file_logger() {
    let log_dir = service_log_dir();
    let _ = std::fs::create_dir_all(&log_dir);

    let timestamp = {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("{}", secs)
    };
    let log_path = log_dir.join(format!("service-{}.log", timestamp));

    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(f) => {
            unsafe { SERVICE_LOG_FILE = Some(f); }
            println!("[SERVICE] Log file: {:?}", log_path);
        }
        Err(e) => {
            eprintln!("[SERVICE] Failed to open log file {:?}: {}", log_path, e);
        }
    }
}

fn log_to_file(msg: &str) {
    let timestamp = {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let (h, m, s) = ((secs % 86400) / 3600, (secs % 3600) / 60, secs % 60);
        format!("{:02}:{:02}:{:02}", h, m, s)
    };
    let line = format!("[{}] {}\n", timestamp, msg);
    if let Some(f) = unsafe { SERVICE_LOG_FILE.as_mut() } {
        let _ = f.write_all(line.as_bytes());
        let _ = f.flush();
    }
    // Also write to stderr for when running in console
    let _ = std::io::stderr().write_all(line.as_bytes());
}

macro_rules! svc_log {
    ($($arg:tt)*) => {
        let msg = format!($($arg)*);
        log_to_file(&msg);
        println!("{}", msg);
    };
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
        let contents = path.parent()?; // .../Contents/MacOS -> .../Contents
        // Standard: <Contents>/Resources/plugins
        let resources = contents.join("Resources");
        let std = resources.join("plugins");
        if std.is_dir() {
            return Some(std);
        }
        // Fallback: <Contents>/Resources/_up_/plugins
        let up = resources.join("_up_").join("plugins");
        if up.is_dir() {
            return Some(up);
        }
        None
    });
    if bundled_dir.is_some() {
        return bundled_dir;
    }

    let dev_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|path| path.join("plugins"));
    dev_dir.filter(|path| path.is_dir())
}

fn resolve_python_path_for_service() {
    let db_path = master_db_path();

    // 1. Check app_settings first
    let from_db = || -> Option<std::path::PathBuf> {
        let conn = rusqlite::Connection::open(&db_path).ok()?;
        let mut stmt = conn.prepare("SELECT value FROM app_settings WHERE key = 'python_path'").ok()?;
        let path: String = stmt.query_row([], |r| r.get(0)).ok()?;
        let p = std::path::PathBuf::from(&path);
        if p.exists() { Some(p) } else { None }
    };

    if let Some(ref path) = from_db() {
        std::env::set_var("PYTHONHOME", path);
        println!("[SERVICE] Using Python at {:?} (from app_settings)", path);
        return;
    }

    // 2. Check if already scanned
    let set_flag = |key: &str, value: &str| {
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            let _ = conn.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
                rusqlite::params![key, value],
            );
        }
    };

    let already_scanned = rusqlite::Connection::open(&db_path).ok().and_then(|conn| {
        conn.prepare("SELECT value FROM app_settings WHERE key = 'python_scanned'")
            .ok()
            .and_then(|mut stmt| stmt.query_row([], |r| r.get::<_, String>(0)).ok())
    }).is_some();

    if already_scanned {
        println!("[SERVICE] Python already scanned and not found. Please install Python or set python_path in settings.");
        return;
    }

    // 3. First scan: try to detect Python from PATH
    println!("[SERVICE] Scanning Python from PATH (first run)...");
    let python_cmd = if cfg!(target_os = "windows") { "python" } else { "python3" };
    let detected = match std::process::Command::new(python_cmd)
        .args(["-c", "import sys; print(sys.prefix)"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                let p = std::path::PathBuf::from(&path);
                if p.exists() { Some(p) } else { None }
            } else { None }
        }
        _ => None,
    };

    if let Some(ref path) = detected {
        set_flag("python_path", &path.to_string_lossy());
        set_flag("python_scanned", "1");
        std::env::set_var("PYTHONHOME", path);
        println!("[SERVICE] Found Python at {:?} — saved to settings", path);
    } else {
        set_flag("python_scanned", "1");
        println!("[SERVICE] Python not found on PATH. Please install Python or set python_path in settings.");
    }
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

    // Eagerly load plugins so their `on_load` hooks run and any registered
    // filters (e.g. oreka's `parsed_data` image filter) are available before
    // the pipeline invokes `call_filter`.
    engine.load_all();

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
            service_control TEXT NOT NULL DEFAULT 'run',
            edit_pid INTEGER,
            cycle_count INTEGER NOT NULL DEFAULT 0,
            last_run_at TEXT,
            last_error TEXT,
            progress_json TEXT,
            ws_port INTEGER,
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

/// Compute realtime progress from the pipeline result + repository aggregate
/// counts, then persist it to the master DB so the GUI can read it.
/// Build a `ServiceProgress` snapshot from the current repository summary.
///
/// When `phase`/`message` are `None` (live ticker), a generic "running" message
/// is used so the GUI shows continuously-updating progress without waiting for
/// the cycle to finish.
fn build_progress(
    _project_id: &str,
    db_path: &PathBuf,
    phase: Option<&str>,
    message: Option<&str>,
    now: &str,
) -> crawlflow_lib::services::ServiceProgress {
    use crawlflow_lib::services::ServiceProgress;

    let summary = crawlflow_lib::repository::RawItemRepository::open(db_path)
        .ok()
        .and_then(|repo| repo.get_summary().ok());

    let (items_total, items_done, items_error, items_pending) = match summary {
        Some(s) => (s.total, s.done, s.error, s.pending + s.processing),
        None => (0, 0, 0, 0),
    };

    let progress_pct = if items_total > 0 {
        (items_done + items_error) as f64 / items_total as f64 * 100.0
    } else {
        0.0
    };

    ServiceProgress {
        items_total,
        items_processed: items_done + items_error,
        items_success: items_done,
        items_failed: items_error,
        items_pending,
        progress_pct,
        phase: phase.unwrap_or("running").to_string(),
        message: message.unwrap_or("Running…").to_string(),
        last_run_at: now.to_string(),
    }
}

fn report_progress(
    project_id: &str,
    db_path: &PathBuf,
    result: &crawlflow_lib::pipeline::RepositoryPipelineResult,
    now: &str,
) {
    use crawlflow_lib::services::ServiceManager;

    let (items_total, items_done) = {
        let summary = crawlflow_lib::repository::RawItemRepository::open(db_path)
            .ok()
            .and_then(|repo| repo.get_summary().ok());
        match summary {
            Some(s) => (s.total, s.done),
            None => (0, 0),
        }
    };

    let phase = if result.success {
        result.phase.clone()
    } else {
        format!("error: {}", result.phase)
    };

    let message = if result.success {
        format!(
            "Cycle ok · ingested={} matched={} processed={} failed={} · done={}/{}",
            result.ingested, result.matched, result.processed, result.failed, items_done, items_total
        )
    } else {
        format!(
            "Cycle failed ({}) · {}",
            result.phase,
            result.error.clone().unwrap_or_default()
        )
    };

    let progress = build_progress(project_id, db_path, Some(&phase), Some(&message), now);
    ServiceManager::write_progress_json(project_id, &progress);
    crawlflow_lib::ws::publish_progress(project_id, serde_json::to_value(&progress).unwrap_or_default());
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

async fn run_project_loop(
    proj: ProjectRow,
    interval_secs: u64,
    shutdown: Arc<AtomicBool>,
    ws_hub: Arc<WsHub>,
) {
    let project_id = proj.id.clone();
    let db_path = project_db_path(&proj.db_path);
    let master_db = master_db_path();

    println!("[SERVICE] Initializing project '{}' ...", proj.name);

    // Ensure project_runtime table exists
    if let Ok(conn) = rusqlite::Connection::open(&master_db) {
        ensure_runtime_table(&conn);
        // The service is taking ownership of execution for this project, so
        // clear any stale edit lock left by the GUI. Without this, a manually
        // launched service + an open GUI would deadlock: the GUI keeps
        // edit_pid set (thinking no service is running) and the service skips
        // every cycle because is_project_being_edited() returns true.
        let _ = conn.execute(
            "UPDATE project_runtime SET edit_pid = NULL, updated_at = datetime('now') WHERE project_id = ?1",
            rusqlite::params![&project_id],
        );
    } else {
        eprintln!("[SERVICE] WARNING: Cannot open master DB at {:?}", master_db);
    }

    println!("[SERVICE] Creating LogManager for project '{}' ...", proj.name);
    println!("[SERVICE-DEBUG-1] Starting LogManager creation...");
    let lm = {
        let db = master_db_path();
        println!("[SERVICE-DEBUG-2] DB path: {:?}", db);
        let mgr = SimpleLogger::as_log_manager(db);
        println!("[SERVICE-DEBUG-3] LogManager created, calling ws::global_hub()...");
        let hub = ws::global_hub();
        println!("[SERVICE-DEBUG-4] ws::global_hub() returned: {:?}", hub.as_ref().map(|_| "Some"));
        if let Some(ref hub) = hub {
            println!("[SERVICE-DEBUG-5] Calling set_ws_hub...");
            mgr.set_ws_hub(hub.clone());
            println!("[SERVICE-DEBUG-6] WS hub attached");
        }
        println!("[SERVICE-DEBUG-7] Wrapping in Arc...");
        Arc::new(mgr)
    };
    println!("[SERVICE-DEBUG-8] LogManager fully initialized");

    // Start (or reuse) this project's realtime WebSocket server and remember
    // its port so logs / progress / per-item events can be pushed live.
    println!("[SERVICE] Starting WebSocket server for project '{}'...", proj.name);
    let ws_port = ws_hub.start_for_project(&project_id).await;
    println!("[SERVICE] WebSocket listening on port {}", ws_port);
    lm.info(
        &project_id,
        "service",
        &format!("[WS] Realtime channel listening on port {}", ws_port),
    );

    // Resolve export settings (global export folder + per-project grouping)
    // once per project loop so the export plugin places files correctly.
    println!("[SERVICE] Resolving export settings for project '{}'...", proj.name);
    let export_settings = get_export_settings(&project_id, &db_path);

    // Resolve refresh strategy (update interval etc.)
    let (refresh_strategy, update_method, refresh_interval) =
        read_project_refresh_strategy(&db_path);
    println!(
        "[SERVICE] Refresh strategy: {} / method: {} / interval: {}s",
        refresh_strategy, update_method, refresh_interval
    );

    println!("[SERVICE] Loading enabled plugins...");
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
    println!("[SERVICE] Initializing Python plugin engine...");
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

    println!("[SERVICE] Loop started for '{}' (every {}s)", proj.name, interval_secs);

    let mut exit_status = "stopped";

    while !shutdown.load(Ordering::Relaxed) {
        cycle += 1;
        println!("[SERVICE] --- Cycle #{} for '{}' ---", cycle, proj.name);
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

        // Check if project has been disabled while the service was running
        let project_enabled = conn_result
            .as_ref()
            .map(|conn| {
                conn.query_row(
                    "SELECT status = 'enabled' FROM projects WHERE id = ?1",
                    rusqlite::params![&project_id],
                    |row| row.get::<_, bool>(0),
                )
                .unwrap_or(false)
            })
            .unwrap_or(false);

        if is_editing || service_control == "paused" || service_control == "stop" || !project_enabled {
            let reason = if !project_enabled {
                "disabled by user"
            } else if is_editing {
                "open in desktop app"
            } else if service_control == "paused" {
                "paused by user"
            } else {
                "stopped by user"
            };
            println!("[SERVICE] Skipping: project '{}' is {}", proj.name, reason);
            lm.info(
                &project_id,
                "service",
                &format!("Project '{}' is {}. Skipping cycle.", proj.name, reason),
            );
            // If stopped or disabled, exit the loop entirely
            if service_control == "stop" || !project_enabled {
                exit_status = if !project_enabled { "disabled" } else { "stopped" };
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

        println!("[SERVICE] Loading pipeline from {:?}...", db_path);
        match load_pipeline(&db_path) {
            Ok((nodes, edges)) => {
                println!("[SERVICE] Pipeline loaded: {} nodes, {} edges. Executing...", nodes.len(), edges.len());
                let config = crawlflow_lib::pipeline::PipelineConfig {
                    nodes,
                    edges,
                    settings: serde_json::json!({
                        "refresh_strategy": refresh_strategy,
                        "update_method": update_method,
                        "refresh_interval": refresh_interval,
                    }),
                };
                // Reset cancellation before execution
                cancellation.store(false, Ordering::Relaxed);

                // Spawn a live progress ticker: while the pipeline runs, sample
                // the repository summary every second and write progress JSON so
                // the GUI receives realtime updates (instead of one snapshot per
                // cycle). The GUI process picks this up and emits a Tauri event.
                let pipeline_running = Arc::new(AtomicBool::new(true));
                let ticker_running = pipeline_running.clone();
                let ticker_pid = project_id.clone();
                let ticker_db = db_path.clone();
                let ticker_lm = lm.clone();
                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
                    while ticker_running.load(Ordering::Relaxed) {
                        interval.tick().await;
                        let now = now_iso();
                        let activity = ticker_lm.latest_activity();
                        let progress = build_progress(&ticker_pid, &ticker_db, None, activity.as_deref(), &now);
                        crawlflow_lib::services::ServiceManager::write_progress_json(&ticker_pid, &progress);
                        // Push live over WebSocket (no polling delay).
                        crawlflow_lib::ws::publish_progress(&ticker_pid, serde_json::to_value(&progress).unwrap_or_default());
                    }
                });

                let result = crawlflow_lib::pipeline::execute_repository_pipeline(
                    &config,
                    &db_path,
                    &lm,
                    &project_id,
                    Some(&mut python_engine),
                    Some(&cancellation),
                )
                .await;
                pipeline_running.store(false, Ordering::Relaxed);
                let now = now_iso();
                println!(
                    "[SERVICE] Cycle #{} result: success={} ingested={} matched={} processed={} failed={} phase={}",
                    cycle, result.success, result.ingested, result.matched, result.processed, result.failed, result.phase
                );
                if let Some(ref err) = result.error {
                    eprintln!("[SERVICE] Cycle #{} error detail: {}", cycle, err);
                }
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

                            // ── Run export processor nodes (e.g. generate-excel-file) ──
                            // The repository phase (worker) only crawls + parses items and
                            // saves final_output into parsed_data. The standalone processor
                            // nodes live in the pipeline graph and are NOT executed by the
                            // repository phase, so we drive them here using the parsed output.
                            if result.processed > 0 {
                                let repo = match crawlflow_lib::repository::RawItemRepository::open(&db_path) {
                                    Ok(r) => {
                                        r.ensure_tables().ok();
                                        r
                                    }
                                    Err(e) => {
                                        lm.error(&project_id, "service", &format!("Cycle #{}: cannot open repo: {}", cycle, e));
                                        continue;
                                    }
                                };
                                let done_items = repo.get_done_items(i64::MAX).unwrap_or_default();
                                let mut export_input: Vec<serde_json::Value> = Vec::new();
                                let mut seen_urls: std::collections::HashSet<String> = std::collections::HashSet::new();
                                for (raw, parsed) in done_items {
                                    let obj = match parsed {
                                        Some(s) => {
                                            let v = serde_json::from_str::<serde_json::Value>(&s)
                                                .ok();
                                            // Plugins may emit a single-element array
                                            // wrapping the product object; unwrap it.
                                            let obj_val = match v {
                                                Some(serde_json::Value::Array(a)) => {
                                                    a.into_iter().next().unwrap_or(
                                                        serde_json::Value::Object(
                                                            serde_json::Map::new(),
                                                        ),
                                                    )
                                                }
                                                Some(other) => other,
                                                None => serde_json::Value::Object(
                                                    serde_json::Map::new(),
                                                ),
                                            };
                                            obj_val
                                                .as_object()
                                                .cloned()
                                                .unwrap_or_else(|| serde_json::Map::new())
                                        }
                                        None => serde_json::Map::new(),
                                    };
                                    let mut rec = obj;
                                    rec.entry("source_url".to_string())
                                        .or_insert_with(|| serde_json::Value::String(raw.source_url.clone()));
                                    // Deduplicate by source_url: keep the first occurrence.
                                    if seen_urls.contains(&raw.source_url) {
                                        continue;
                                    }
                                    seen_urls.insert(raw.source_url);
                                    export_input.push(serde_json::Value::Object(rec));
                                }
                                lm.info(
                                    &project_id,
                                    "service",
                                    &format!(
                                        "Cycle #{}: export_input built = {} items; sample keys = {:?}; sample product_name = {:?}",
                                        cycle,
                                        export_input.len(),
                                        export_input.first().and_then(|v| v.as_object()).map(|o| o.keys().cloned().collect::<Vec<String>>()),
                                        export_input.first().and_then(|v| v.get("product_name")).map(|v| v.to_string()),
                                    ),
                                );

                                for node in &config.nodes {
                                    if node.node_type != "processor" {
                                        continue;
                                    }
                                    let ptype = node
                                        .data
                                        .get("processorType")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    if !crawlflow_lib::plugins::is_export_processor(ptype) {
                                        continue;
                                    }
                                    let mut pconfig = node
                                        .data
                                        .get("processorConfig")
                                        .cloned()
                                        .or_else(|| node.data.get("settings").cloned())
                                        .or_else(|| node.data.get("config").cloned())
                                        .unwrap_or(serde_json::Value::Null);
                                    if let Some(obj) = pconfig.as_object_mut() {
                                        if !obj.contains_key("extractFields") {
                                            let extract_fields: Vec<String> = config
                                                .nodes
                                                .iter()
                                                .filter(|n| n.node_type == "html-data-extractor")
                                                .filter_map(|n| {
                                                    let rules = n
                                                        .data
                                                        .get("customRules")
                                                        .or_else(|| n.data.get("extractionRules"))
                                                        .or_else(|| n.data.get("extractRules"))?
                                                        .as_array()?;
                                                    Some(
                                                        rules
                                                            .iter()
                                                            .filter_map(|r| {
                                                                r.get("name")
                                                                    .and_then(|v| v.as_str())
                                                                    .map(|s| s.to_string())
                                                            })
                                                            .collect::<Vec<String>>(),
                                                    )
                                                })
                                                .flatten()
                                                .collect();
                                            if !extract_fields.is_empty() {
                                                obj.insert(
                                                    "extractFields".into(),
                                                    serde_json::json!(extract_fields),
                                                );
                                            }
                                        }
                                        // Inject export settings so the plugin
                                        // writes to the chosen folder and groups
                                        // per project when enabled.
                                        if let Some(dir) = &export_settings.export_dir {
                                            obj.insert(
                                                "outputDir".into(),
                                                serde_json::json!(dir),
                                            );
                                        }
                                        obj.insert(
                                            "groupExport".into(),
                                            serde_json::json!(export_settings.group_export),
                                        );
                                        obj.insert(
                                            "groupFormat".into(),
                                            serde_json::json!(export_settings.group_format),
                                        );
                                        obj.insert(
                                            "projectId".into(),
                                            serde_json::json!(project_id.clone()),
                                        );
                                        obj.insert(
                                            "projectName".into(),
                                            serde_json::json!(proj.name.clone()),
                                        );
                                    }
                                    match crawlflow_lib::plugins::excel_export_plugin(
                                        export_input.clone(),
                                        pconfig,
                                    ) {
                                        Ok(out) => {
                                            let file = out
                                                .first()
                                                .and_then(|v| v.get("file"))
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("<unknown>");
                                            lm.info(
                                                &project_id,
                                                "service",
                                                &format!(
                                                    "Cycle #{}: export wrote {} items -> {}",
                                                    cycle,
                                                    export_input.len(),
                                                    file
                                                ),
                                            );
                                        }
                                        Err(e) => {
                                            lm.error(
                                                &project_id,
                                                "service",
                                                &format!("Cycle #{}: export failed: {}", cycle, e),
                                            );
                                        }
                                    }
                                }
                            }

                            // For 'update_only' strategy: stop after 1 successful cycle.
                            // For 'refresh' / 'refresh_update': keep looping.
                            if refresh_strategy == "update_only" {
                                exit_status = "completed";
                                let _ = conn.execute(
                                    "UPDATE project_runtime SET service_control = 'stop' WHERE project_id = ?1",
                                    rusqlite::params![&project_id],
                                );
                            }
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

                // ── Report realtime progress to the master DB (read by the GUI) ──
                report_progress(&project_id, &db_path, &result, &now);
            }
            Err(e) => {
                eprintln!("[SERVICE] ERROR: Load pipeline failed: {}", e);
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

    // Mark status on exit
    if let Ok(conn) = rusqlite::Connection::open(&master_db) {
        set_runner_status(&conn, &project_id, exit_status, None, None, None, None);
    }
    lm.info(&project_id, "service", &format!("Loop stopped (status={}).", exit_status));
}

// ── Entry point ───────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--service") {
        run_as_windows_service();
    } else {
        run_as_console(&args);
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    run_as_console(&args);
}

// ── Console mode (manual launch / dev) ───────────────────────────────────────────

fn run_as_console(args: &[String]) {
    let mut target_project: Option<String> = None;
    let mut run_all = false;
    let mut interval_secs: u64 = 60;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--project" => { i += 1; target_project = args.get(i).cloned(); }
            "--all" => { run_all = true; }
            "--interval" => { i += 1; interval_secs = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(60); }
            "--service" => {}
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

    println!("[SERVICE] CrawlFlow background service starting (console mode)");
    println!("[SERVICE] Data dir : {:?}", get_app_data_dir());
    println!("[SERVICE] Master DB: {:?}", master_db_path());
    println!("[SERVICE] Interval : {}s", interval_secs);

    resolve_python_path_for_service();

    let shutdown = Arc::new(AtomicBool::new(false));
    {
        let flag = shutdown.clone();
        ctrlc::set_handler(move || {
            println!("\n[SERVICE] Shutdown signal — stopping...");
            flag.store(true, Ordering::Relaxed);
        })
        .ok();
    }

    let ws_hub = WsHub::new();
    {
        let master = master_db_path();
        ws_hub.set_port_persister(Box::new(move |project_id, port| {
            if let Ok(conn) = rusqlite::Connection::open(&master) {
                let _ = conn.execute(
                    "INSERT INTO project_runtime (project_id, ws_port, updated_at)
                     VALUES (?1, ?2, datetime('now'))
                     ON CONFLICT(project_id) DO UPDATE SET ws_port = ?2, updated_at = datetime('now')",
                    rusqlite::params![project_id, port as i64],
                );
            }
        }));
    }
    ws::set_global_hub(ws_hub.clone());

    let all_projects = match list_enabled_projects() {
        Ok(p) => p,
        Err(e) => { eprintln!("[SERVICE] DB error: {}", e); std::process::exit(1); }
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

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(async {
        let mut handles = Vec::new();
        for proj in projects {
            let sd = shutdown.clone();
            let hub = ws_hub.clone();
            handles.push(tokio::spawn(run_project_loop(proj, interval_secs, sd, hub)));
        }
        while !shutdown.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        println!("[SERVICE] Shutting down project loops...");
        for h in handles { let _ = h.await; }
    });

    crawlflow_lib::request_clients::shutdown_global_chrome();
    println!("[SERVICE] All stopped. Goodbye.");
}

// ── Windows Service mode (launched by SCM) ───────────────────────────────────────

#[cfg(target_os = "windows")]
const SERVICE_NAME: &str = "CrawlFlowService";

#[cfg(target_os = "windows")]
fn run_as_windows_service() {
    use windows_service::define_windows_service;
    use windows_service::service_dispatcher;

    define_windows_service!(ffi_service_main, service_main_entry);

    if let Err(e) = service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
        eprintln!("[SERVICE] Failed to connect to SCM: {:?}", e);
        eprintln!("[SERVICE] Falling back to console mode.");
        let args: Vec<String> = std::env::args().collect();
        let filtered: Vec<String> = args.into_iter().filter(|a| a != "--service").collect();
        run_as_console(&filtered);
    }
}

#[cfg(target_os = "windows")]
fn service_main_entry(_arguments: Vec<std::ffi::OsString>) {
    use windows_service::service::{
        ServiceStatus, ServiceType, ServiceState, ServiceControlAccept,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    // Initialize file logger first — all svc_log! calls write here
    init_file_logger();

    // Catch panics so we always write to the log file
    std::panic::set_hook(Box::new(|info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            format!("PANIC: {}", s)
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            format!("PANIC: {}", s)
        } else {
            format!("PANIC: {:?}", info)
        };
        if let Some(loc) = info.location() {
            log_to_file(&format!("{} at {}:{}", msg, loc.file(), loc.line()));
        } else {
            log_to_file(&msg);
        }
    }));

    svc_log!("[SERVICE] CrawlFlow Windows Service starting");
    svc_log!("[SERVICE] Data dir : {:?}", get_app_data_dir());
    svc_log!("[SERVICE] Master DB: {:?}", master_db_path());
    svc_log!("[SERVICE] Log dir  : {:?}", service_log_dir());

    resolve_python_path_for_service();

    let shutdown = Arc::new(AtomicBool::new(false));

    // Register control handler via closure
    let shutdown_for_handler = shutdown.clone();
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            windows_service::service::ServiceControl::Stop => {
                svc_log!("[SERVICE] SCM stop signal received");
                shutdown_for_handler.store(true, Ordering::Relaxed);
                ServiceControlHandlerResult::NoError
            }
            windows_service::service::ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };
    let status_handle = match service_control_handler::register(SERVICE_NAME, event_handler) {
        Ok(h) => h,
        Err(e) => {
            svc_log!("[SERVICE] FATAL: Failed to register handler: {:?}", e);
            return;
        }
    };

    // Report start pending
    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: windows_service::service::ServiceExitCode::ServiceSpecific(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::from_secs(10),
        process_id: None,
    });

    svc_log!("[SERVICE] Initializing WebSocket hub...");
    let ws_hub = WsHub::new();
    {
        let master = master_db_path();
        ws_hub.set_port_persister(Box::new(move |project_id, port| {
            if let Ok(conn) = rusqlite::Connection::open(&master) {
                let _ = conn.execute(
                    "INSERT INTO project_runtime (project_id, ws_port, updated_at)
                     VALUES (?1, ?2, datetime('now'))
                     ON CONFLICT(project_id) DO UPDATE SET ws_port = ?2, updated_at = datetime('now')",
                    rusqlite::params![project_id, port as i64],
                );
            }
        }));
    }
    ws::set_global_hub(ws_hub.clone());

    svc_log!("[SERVICE] Querying enabled projects...");
    let all_projects = match list_enabled_projects() {
        Ok(p) => p,
        Err(e) => {
            svc_log!("[SERVICE] FATAL: DB error: {}", e);
            let _ = status_handle.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Stopped,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code: windows_service::service::ServiceExitCode::ServiceSpecific(1),
                checkpoint: 0,
                wait_hint: std::time::Duration::ZERO,
                process_id: None,
            });
            return;
        }
    };

    if all_projects.is_empty() {
        svc_log!("[SERVICE] No enabled projects found. Stopping.");
        let _ = status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: windows_service::service::ServiceExitCode::ServiceSpecific(0),
            checkpoint: 0,
            wait_hint: std::time::Duration::ZERO,
            process_id: None,
        });
        return;
    }

    svc_log!("[SERVICE] Starting {} enabled project(s)", all_projects.len());

    // Report running
    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: windows_service::service::ServiceExitCode::ServiceSpecific(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::ZERO,
        process_id: None,
    });

    let interval_secs: u64 = 60;
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            svc_log!("[SERVICE] FATAL: Failed to create tokio runtime: {}", e);
            return;
        }
    };
    rt.block_on(async {
        let mut handles = Vec::new();
        for proj in all_projects {
            svc_log!("[SERVICE] Spawning loop for project '{}' ({})", proj.name, proj.id);
            let sd = shutdown.clone();
            let hub = ws_hub.clone();
            handles.push(tokio::spawn(run_project_loop(proj, interval_secs, sd, hub)));
        }
        while !shutdown.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        svc_log!("[SERVICE] Stop signal received. Shutting down project loops...");
        for h in handles { let _ = h.await; }
    });

    crawlflow_lib::request_clients::shutdown_global_chrome();

    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: windows_service::service::ServiceExitCode::ServiceSpecific(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::ZERO,
        process_id: None,
    });
    svc_log!("[SERVICE] All stopped. Goodbye.");
}
