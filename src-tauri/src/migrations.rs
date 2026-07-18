use tauri_plugin_sql::{Migration, MigrationKind};

pub fn get_master_migrations() -> Vec<Migration> {
    vec![Migration {
        version: 4,
        description: "Create projects, app_settings, extensions, project_runtime if missing",
        sql: r#"
            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                db_path TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'disabled',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS extensions (
                id TEXT PRIMARY KEY,
                type TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT DEFAULT '',
                version TEXT DEFAULT '1.0.0',
                enabled INTEGER NOT NULL DEFAULT 1,
                installed_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS project_runtime (
                project_id TEXT PRIMARY KEY,
                runner_status TEXT NOT NULL DEFAULT 'stopped',
                runner_pid INTEGER,
                runner_type TEXT DEFAULT 'service',
                service_control TEXT NOT NULL DEFAULT 'run',
                edit_pid INTEGER,
                cycle_count INTEGER NOT NULL DEFAULT 0,
                last_run_at TEXT,
                last_error TEXT,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS raw_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_url TEXT NOT NULL,
                item_type TEXT NOT NULL DEFAULT 'url',
                item_hash TEXT NOT NULL,
                extracted_url TEXT,
                dup_count INTEGER NOT NULL DEFAULT 1,
                priority INTEGER NOT NULL DEFAULT 0,
                worker_id TEXT,
                matched INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_raw_items_hash ON raw_items(item_hash);
            CREATE INDEX IF NOT EXISTS idx_raw_items_status ON raw_items(status);
            CREATE INDEX IF NOT EXISTS idx_raw_items_matched ON raw_items(matched);
            CREATE INDEX IF NOT EXISTS idx_raw_items_worker ON raw_items(worker_id);

            CREATE TABLE IF NOT EXISTS crawl_data (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                raw_item_id INTEGER NOT NULL,
                content_type TEXT NOT NULL DEFAULT 'raw',
                content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (raw_item_id) REFERENCES raw_items(id)
            );

            CREATE INDEX IF NOT EXISTS idx_crawl_data_raw_item ON crawl_data(raw_item_id);

            CREATE TABLE IF NOT EXISTS json_ld (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                raw_item_id INTEGER NOT NULL,
                ld_type TEXT NOT NULL DEFAULT 'unknown',
                data TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (raw_item_id) REFERENCES raw_items(id)
            );

            CREATE INDEX IF NOT EXISTS idx_json_ld_raw_item ON json_ld(raw_item_id);

            CREATE TABLE IF NOT EXISTS parsed_data (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                raw_item_id INTEGER NOT NULL,
                worker_id TEXT,
                processor_id TEXT NOT NULL,
                data TEXT NOT NULL,
                is_final INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'done',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (raw_item_id) REFERENCES raw_items(id)
            );

            CREATE INDEX IF NOT EXISTS idx_parsed_data_raw_item ON parsed_data(raw_item_id);
            CREATE INDEX IF NOT EXISTS idx_parsed_data_worker ON parsed_data(worker_id);
            CREATE INDEX IF NOT EXISTS idx_parsed_data_processor ON parsed_data(processor_id);
            CREATE INDEX IF NOT EXISTS idx_parsed_data_final ON parsed_data(is_final);

            CREATE TABLE IF NOT EXISTS process_request_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                worker_id TEXT NOT NULL,
                raw_item_id INTEGER NOT NULL,
                processor_id TEXT NOT NULL,
                processor_type TEXT NOT NULL,
                input_data TEXT,
                input_config TEXT,
                output_data TEXT,
                retry_count INTEGER DEFAULT 0,
                max_retry INTEGER DEFAULT 3,
                status TEXT NOT NULL DEFAULT "pending",
                error_message TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime("now")),
                finished_at TEXT,
                FOREIGN KEY (raw_item_id) REFERENCES raw_items(id),
                FOREIGN KEY (worker_id) REFERENCES workers(id)
            );

            CREATE INDEX IF NOT EXISTS idx_process_request_history_worker ON process_request_history(worker_id);
            CREATE INDEX IF NOT EXISTS idx_process_request_history_item ON process_request_history(raw_item_id);
            CREATE INDEX IF NOT EXISTS idx_process_request_history_status ON process_request_history(status);
        "#,
        kind: MigrationKind::Up,
    }]
}
