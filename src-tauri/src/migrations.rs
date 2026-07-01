use tauri_plugin_sql::{Migration, MigrationKind};

pub fn get_master_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "create master database tables",
            sql: "
                CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT DEFAULT '',
                    status TEXT NOT NULL DEFAULT 'draft',
                    db_path TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS extensions (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT,
                    type TEXT NOT NULL,
                    config TEXT NOT NULL DEFAULT '{}',
                    enabled INTEGER NOT NULL DEFAULT 1,
                    installed_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS app_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
            ",
            kind: MigrationKind::Up,
        },
    ]
}

#[allow(dead_code)]
pub fn get_project_migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "create project database tables",
            sql: "
                CREATE TABLE IF NOT EXISTS project_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS nodes (
                    id TEXT PRIMARY KEY,
                    type TEXT NOT NULL,
                    label TEXT,
                    position_x REAL NOT NULL DEFAULT 0,
                    position_y REAL NOT NULL DEFAULT 0,
                    data TEXT NOT NULL DEFAULT '{}',
                    deletable INTEGER NOT NULL DEFAULT 1,
                    draggable INTEGER NOT NULL DEFAULT 1,
                    width REAL,
                    height REAL,
                    z_index INTEGER DEFAULT 0,
                    parent_node TEXT,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS edges (
                    id TEXT PRIMARY KEY,
                    source TEXT NOT NULL,
                    target TEXT NOT NULL,
                    source_handle TEXT,
                    target_handle TEXT,
                    type TEXT DEFAULT 'smoothstep',
                    animated INTEGER NOT NULL DEFAULT 0,
                    data TEXT DEFAULT '{}',
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS crawl_data (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_url TEXT,
                    field_name TEXT NOT NULL,
                    field_value TEXT,
                    raw_data TEXT,
                    node_id TEXT,
                    extracted_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS crawl_logs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    level TEXT NOT NULL DEFAULT 'info',
                    message TEXT NOT NULL,
                    node_id TEXT,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type);
                CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
                CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);
                CREATE INDEX IF NOT EXISTS idx_crawl_data_field ON crawl_data(field_name);
            ",
            kind: MigrationKind::Up,
        },
    ]
}
