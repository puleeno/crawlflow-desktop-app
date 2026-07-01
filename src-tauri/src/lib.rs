mod commands;
mod crawler;
mod migrations;
mod models;
mod plugins;
mod python_plugins;

use commands::AppState;
use plugins::PluginEngine;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

fn resolve_plugin_dir(_app: &tauri::App) -> PathBuf {
    let home = dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("crawlflow")
        .join("plugins");

    // Ensure the directory exists
    std::fs::create_dir_all(&home).ok();

    // Also try app-local resource dir for bundled plugins
    home
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut engine = PluginEngine::new(PathBuf::from(".")); // placeholder, updated in setup
    plugins::register_builtin_plugins(&mut engine);

    tauri::Builder::default()
        .manage(AppState {
            plugin_engine: Mutex::new(engine),
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
            commands::parse_html_table_cmd,
            // Python plugin commands
            commands::list_python_plugins_cmd,
            commands::execute_python_hook_cmd,
            commands::call_python_data_source_cmd,
            commands::call_python_export_cmd,
            commands::run_python_pipeline_cmd,
            commands::reload_python_plugins_cmd,
        ])
        .setup(|app| {
            // Resolve plugin directory
            let plugin_dir = resolve_plugin_dir(app);
            log::info!("Python plugin directory: {:?}", plugin_dir);

            // Replace the managed state engine with the correct plugin dir
            let state: tauri::State<'_, AppState> = app.state();
            let mut guard = state.plugin_engine.lock().unwrap();
            *guard = PluginEngine::new(plugin_dir);
            plugins::register_builtin_plugins(&mut *guard);

            match guard.init_python_plugins() {
                Ok(discovered) => log::info!("Python plugins initialized: {:?}", discovered),
                Err(e) => log::warn!("Python plugin init (ok to ignore): {}", e),
            }
            drop(guard);

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
