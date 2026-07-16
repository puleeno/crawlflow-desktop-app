mod commands;
mod crawler;
pub mod data_preprocessor;
mod finish_actions;
mod item_matcher;
pub mod logs;
mod migrations;
mod models;
pub mod pagination;
pub mod pipeline;
mod plugins;
mod progress;
pub mod python_plugins;
mod repository;
pub mod request_clients;
mod services;
pub mod settings_engine;
pub mod spreadsheet;
mod system_service;
mod worker_engine;

use commands::AppState;
use logs::LogManager;
use plugins::PluginEngine;
use services::ServiceManager;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Manager;

fn get_user_plugins_dir() -> PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.CrawlFlow.desktop")
        .join("plugins")
}

fn get_builtin_plugins_dir(app: &tauri::App) -> Option<PathBuf> {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let path = resource_dir.join("plugins");
        if path.is_dir() {
            return Some(path);
        }
    }
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("plugins"))
        .unwrap_or_default();
    if dev_path.is_dir() {
        return Some(dev_path);
    }
    None
}

fn is_service_mode() -> bool {
    std::env::args().any(|a| a == "--service")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_manager = Arc::new(LogManager::new());
    let _initial_plugin_engine = {
        let mut e = PluginEngine::new(None, get_user_plugins_dir());
        plugins::register_builtin_plugins(&mut e);
        e
    };

    // Build service_manager stub — real init happens in setup
    let service_manager = Arc::new(ServiceManager::new_uninitialized());

    tauri::Builder::default()
        .manage(AppState {
            plugin_engine: Mutex::new(PluginEngine::new(None, get_user_plugins_dir())),
            log_manager: log_manager.clone(),
            service_manager: service_manager.clone(),
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:crawlflow.db", migrations::get_master_migrations())
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::fetch_url_cmd,
            commands::batch_crawl_cmd,
            commands::extract_html_cmd,
            commands::execute_processor_cmd,
            commands::list_plugins_cmd,
            commands::execute_batch_processor_cmd,
            commands::fetch_rss_cmd,
            commands::export_csv_cmd,
            commands::export_excel_cmd,
            commands::spreadsheet_read_cmd,
            commands::spreadsheet_write_cmd,
            commands::spreadsheet_export_cmd,
            commands::parse_html_table_cmd,
            commands::list_python_plugins_cmd,
            commands::execute_python_hook_cmd,
            commands::call_python_data_source_cmd,
            commands::call_python_filter_cmd,
            commands::call_python_export_cmd,
            commands::run_python_pipeline_cmd,
            commands::reload_python_plugins_cmd,
            commands::parse_html_with_bs4_cmd,
            commands::summarize_parsed_html_cmd,
            commands::install_marketplace_item,
            commands::list_presets_cmd,
            commands::run_demo_cmd,
            // Progress
            commands::get_project_progress_cmd,
            // Request client
            commands::fetch_with_client_cmd,
            // Service commands
            commands::start_project_service_cmd,
            commands::stop_project_service_cmd,
            commands::pause_project_service_cmd,
            commands::resume_project_service_cmd,
            commands::get_service_status_cmd,
            commands::list_project_services_cmd,
            // Log commands
            commands::get_project_logs_cmd,
            commands::clear_project_logs_cmd,
            // System service commands
            commands::get_service_install_info_cmd,
            commands::install_system_service_cmd,
            commands::uninstall_system_service_cmd,
            commands::start_system_service_cmd,
            commands::stop_system_service_cmd,
            // Settings engine
            commands::list_processor_settings_schemas,
            commands::get_processor_settings_schema,
            commands::validate_settings_values,
            commands::get_settings_defaults,
            // Raw items browser
            commands::get_raw_items_cmd,
            commands::get_raw_items_summary_cmd,
            // App settings
            commands::get_app_setting_cmd,
            commands::set_app_setting_cmd,
            commands::lock_project_edit_cmd,
            commands::unlock_project_edit_cmd,
            commands::request_project_run_cmd,
            commands::request_project_stop_cmd,
        ])
        .setup(|app| {
            let user_dir = get_user_plugins_dir();
            let builtin_dir = get_builtin_plugins_dir(app);

            // Clear any stale edit locks
            if let Ok(dir) = app.path().app_data_dir() {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().map_or(false, |ext| ext == "edit") {
                            let _ = std::fs::remove_file(path);
                        }
                    }
                }
            }

            std::fs::create_dir_all(&user_dir).ok();
            if let Some(ref bd) = builtin_dir {
                log::info!("Built-in plugin directory: {:?}", bd);
            }
            log::info!("User plugin directory: {:?}", user_dir);

            let state: tauri::State<'_, AppState> = app.state();

            // Initialize plugin engine
            {
                let mut guard = state.plugin_engine.lock().unwrap();
                *guard = PluginEngine::new(builtin_dir.clone(), user_dir);
                plugins::register_builtin_plugins(&mut *guard);
                match guard.init_python_plugins() {
                    Ok(discovered) => log::info!("Python plugins initialized: {:?}", discovered),
                    Err(e) => log::warn!("Python plugin init (ok to ignore): {}", e),
                }
            }

            // Initialize log manager with app handle and DB path for persistence
            let app_handle = app.handle().clone();
            state.log_manager.set_app_handle(app_handle.clone());
            {
                let db_path = dirs_next::data_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("com.CrawlFlow.desktop")
                    .join("crawlflow.db");
                state.log_manager.set_master_db_path(db_path);
            }

            // Initialize service manager
            state
                .service_manager
                .initialize(app_handle, state.log_manager.clone());

            // Create window only in GUI mode (--service flag = headless)
            if !is_service_mode() {
                let _window = tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("index.html".into()),
                )
                .title("CrawlFlow Desktop")
                .inner_size(1280.0, 800.0)
                .min_inner_size(900.0, 600.0)
                .resizable(true)
                .build()?;
            }

            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
