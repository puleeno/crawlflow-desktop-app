use tauri_plugin_sql::{Migration, MigrationKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_migrations_version() {
        let migrations = get_master_migrations();
        assert!(!migrations.is_empty());
        assert_eq!(migrations[0].version, 1);
        assert!(format!("{:?}", migrations[0].kind).contains("Up"));
    }

    #[test]
    fn test_master_migrations_contains_projects_table() {
        let migrations = get_master_migrations();
        let sql = &migrations[0].sql;
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS projects"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS extensions"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS app_settings"));
    }

    #[test]
    fn test_master_migrations_has_project_id_pk() {
        let migrations = get_master_migrations();
        let sql = &migrations[0].sql;
        assert!(sql.contains("id TEXT PRIMARY KEY"));
    }

    #[test]
    fn test_project_migrations_version() {
        let migrations = get_project_migrations();
        assert!(!migrations.is_empty());
        assert_eq!(migrations[0].version, 1);
    }

    #[test]
    fn test_project_migrations_contains_all_tables() {
        let migrations = get_project_migrations();
        let sql = &migrations[0].sql;
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS project_settings"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS nodes"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS edges"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS crawl_data"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS crawl_logs"));
    }

    #[test]
    fn test_project_migrations_has_indexes() {
        let migrations = get_project_migrations();
        let sql = &migrations[0].sql;
        assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_nodes_type"));
        assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_edges_source"));
        assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_edges_target"));
        assert!(sql.contains("CREATE INDEX IF NOT EXISTS idx_crawl_data_field"));
    }

    #[test]
    fn test_master_migrations_description() {
        let migrations = get_master_migrations();
        assert_eq!(migrations[0].description, "create master database tables");
    }

    #[test]
    fn test_project_migrations_description() {
        let migrations = get_project_migrations();
        assert_eq!(migrations[0].description, "create project database tables");
    }

    #[test]
    fn test_crawl_data_columns() {
        let migrations = get_project_migrations();
        let sql = &migrations[0].sql;
        assert!(sql.contains("source_url TEXT"));
        assert!(sql.contains("field_name TEXT NOT NULL"));
        assert!(sql.contains("field_value TEXT"));
        assert!(sql.contains("raw_data TEXT"));
        assert!(sql.contains("node_id TEXT"));
        assert!(sql.contains("extracted_at TEXT"));
    }

    #[test]
    fn test_nodes_has_position_columns() {
        let migrations = get_project_migrations();
        let sql = &migrations[0].sql;
        assert!(sql.contains("position_x REAL"));
        assert!(sql.contains("position_y REAL"));
        assert!(sql.contains("deletable INTEGER"));
    }
}

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

                CREATE TABLE IF NOT EXISTS project_runtime (
                    project_id TEXT PRIMARY KEY,
                    runner_status TEXT NOT NULL DEFAULT 'stopped',
                    runner_pid INTEGER,
                    runner_type TEXT DEFAULT 'service',
                    edit_pid INTEGER,
                    service_control TEXT NOT NULL DEFAULT 'run',
                    cycle_count INTEGER NOT NULL DEFAULT 0,
                    last_run_at TEXT,
                    last_error TEXT,
                    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE INDEX IF NOT EXISTS idx_project_runtime_status ON project_runtime(runner_status);
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

                CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(type);
                CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
                CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);
            ",
            kind: MigrationKind::Up,
        },
    ]
}

pub fn get_project_migrations_v2() -> Vec<Migration> {
    vec![
        Migration {
            version: 2,
            description: "create raw_items and processing_log tables",
            sql: "
                CREATE TABLE IF NOT EXISTS raw_items (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_url TEXT NOT NULL,
                    item_type TEXT NOT NULL DEFAULT 'url',
                    item_hash TEXT NOT NULL,
                    raw_content TEXT,
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

                CREATE TABLE IF NOT EXISTS processing_log (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    item_id INTEGER NOT NULL,
                    worker_id TEXT,
                    processor_type TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    output TEXT,
                    error TEXT,
                    started_at TEXT,
                    finished_at TEXT,
                    created_at TEXT NOT NULL DEFAULT (datetime('now'))
                );

                CREATE INDEX IF NOT EXISTS idx_processing_log_item ON processing_log(item_id);
                CREATE INDEX IF NOT EXISTS idx_processing_log_worker ON processing_log(worker_id);
            ",
            kind: MigrationKind::Up,
        },
    ]
}
