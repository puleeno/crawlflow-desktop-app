use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ── Data Types ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawItem {
    pub id: i64,
    pub source_url: String,
    pub item_type: String,
    pub item_hash: String,
    pub raw_content: Option<String>,
    pub extracted_url: Option<String>,
    pub dup_count: i32,
    pub priority: i32,
    pub worker_id: Option<String>,
    pub matched: i32,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRawItem {
    pub source_url: String,
    pub item_type: String,
    pub item_hash: String,
    pub raw_content: Option<String>,
    pub extracted_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingLogEntry {
    pub id: i64,
    pub item_id: i64,
    pub worker_id: Option<String>,
    pub processor_type: String,
    pub status: String,
    pub output: Option<String>,
    pub error: Option<String>,
}

// ── Repository ────────────────────────────────────────────

pub struct RawItemRepository {
    conn: Connection,
}

impl RawItemRepository {
    pub fn open(db_path: &Path) -> Result<Self, String> {
        let conn = Connection::open(db_path)
            .map_err(|e| format!("Failed to open repository DB: {}", e))?;
        Ok(Self { conn })
    }

    pub fn ensure_tables(&self) -> Result<(), String> {
        self.conn.execute_batch("
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
        ").map_err(|e| format!("Failed to create tables: {}", e))
    }

    /// Save items with dedup. Increments dup_count for existing items.
    pub fn save_items(&self, items: &[NewRawItem]) -> Result<RawItemSaveResult, String> {
        let mut inserted = 0i64;
        let mut duplicated = 0i64;
        let mut skipped = 0i64;

        for item in items {
            // Check existing by hash
            let existing: Option<i64> = self.conn
                .query_row(
                    "SELECT id FROM raw_items WHERE item_hash = ?1",
                    params![item.item_hash],
                    |row| row.get(0),
                )
                .ok();

            match existing {
                Some(id) => {
                    // Increment dup_count and recalculate priority
                    self.conn.execute(
                        "UPDATE raw_items SET dup_count = dup_count + 1,
                         priority = dup_count + 1,
                         updated_at = datetime('now')
                         WHERE id = ?1",
                        params![id],
                    ).map_err(|e| format!("Failed to update dup_count: {}", e))?;
                    duplicated += 1;
                }
                None => {
                    let priority = if item.extracted_url.is_some() { 5 } else { 1 };
                    self.conn.execute(
                        "INSERT INTO raw_items (source_url, item_type, item_hash, raw_content, extracted_url, priority)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            item.source_url,
                            item.item_type,
                            item.item_hash,
                            item.raw_content,
                            item.extracted_url,
                            priority,
                        ],
                    ).map_err(|e| format!("Failed to insert item: {}", e))?;
                    inserted += 1;
                }
            }
        }

        Ok(RawItemSaveResult { inserted, duplicated, skipped })
    }

    /// Lấy các items pending (chưa xử lý)
    pub fn get_pending_items(&self, limit: i64) -> Result<Vec<RawItem>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_url, item_type, item_hash, raw_content, extracted_url,
                    dup_count, priority, worker_id, matched, status, created_at, updated_at
             FROM raw_items
             WHERE status = 'pending' AND matched = 0
             ORDER BY priority DESC, dup_count DESC
             LIMIT ?1"
        ).map_err(|e| format!("Failed to prepare query: {}", e))?;

        let items = stmt.query_map(params![limit], |row| {
            Ok(RawItem {
                id: row.get(0)?,
                source_url: row.get(1)?,
                item_type: row.get(2)?,
                item_hash: row.get(3)?,
                raw_content: row.get(4)?,
                extracted_url: row.get(5)?,
                dup_count: row.get(6)?,
                priority: row.get(7)?,
                worker_id: row.get(8)?,
                matched: row.get(9)?,
                status: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        }).map_err(|e| format!("Failed to query items: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(items)
    }

    /// Lấy items đã match với worker
    pub fn get_matched_items(&self, worker_id: &str, limit: i64) -> Result<Vec<RawItem>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_url, item_type, item_hash, raw_content, extracted_url,
                    dup_count, priority, worker_id, matched, status, created_at, updated_at
             FROM raw_items
             WHERE worker_id = ?1 AND status = 'pending' AND matched = 1
             ORDER BY priority DESC
             LIMIT ?2"
        ).map_err(|e| format!("Failed to prepare query: {}", e))?;

        let items = stmt.query_map(params![worker_id, limit], |row| {
            Ok(RawItem {
                id: row.get(0)?,
                source_url: row.get(1)?,
                item_type: row.get(2)?,
                item_hash: row.get(3)?,
                raw_content: row.get(4)?,
                extracted_url: row.get(5)?,
                dup_count: row.get(6)?,
                priority: row.get(7)?,
                worker_id: row.get(8)?,
                matched: row.get(9)?,
                status: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        }).map_err(|e| format!("Failed to query items: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(items)
    }

    /// Gán worker cho item
    pub fn assign_worker(&self, item_id: i64, worker_id: &str) -> Result<(), String> {
        self.conn.execute(
            "UPDATE raw_items SET worker_id = ?1, matched = 1, updated_at = datetime('now')
             WHERE id = ?2",
            params![worker_id, item_id],
        ).map_err(|e| format!("Failed to assign worker: {}", e))?;
        Ok(())
    }

    /// Set ignore cho items pending nhung khong match worker nao
    pub fn ignore_unmatched(&self) -> Result<i64, String> {
        let count = self.conn.execute(
            "UPDATE raw_items SET matched = -1, status = 'ignored', updated_at = datetime('now')
             WHERE status = 'pending' AND matched = 0",
            [],
        ).map_err(|e| format!("Failed to ignore unmatched: {}", e))?;
        Ok(count as i64)
    }

    /// Cap nhat status cho item
    pub fn update_status(&self, item_id: i64, status: &str) -> Result<(), String> {
        self.conn.execute(
            "UPDATE raw_items SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![status, item_id],
        ).map_err(|e| format!("Failed to update status: {}", e))?;
        Ok(())
    }

    /// Log processing step
    pub fn log_processing(&self, item_id: i64, worker_id: Option<&str>,
                          processor_type: &str, status: &str,
                          output: Option<&str>, error: Option<&str>) -> Result<(), String> {
        self.conn.execute(
            "INSERT INTO processing_log (item_id, worker_id, processor_type, status, output, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![item_id, worker_id, processor_type, status, output, error],
        ).map_err(|e| format!("Failed to log processing: {}", e))?;
        Ok(())
    }

    /// Dem so items theo status
    pub fn count_by_status(&self, status: &str) -> Result<i64, String> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM raw_items WHERE status = ?1",
            params![status],
            |row| row.get(0),
        ).map_err(|e| format!("Failed to count items: {}", e))
    }

    /// Reset items pending lai sau khi xử lý lỗi
    pub fn reset_failed_items(&self) -> Result<i64, String> {
        let count = self.conn.execute(
            "UPDATE raw_items SET status = 'pending', updated_at = datetime('now')
             WHERE status = 'error'",
            [],
        ).map_err(|e| format!("Failed to reset failed items: {}", e))?;
        Ok(count as i64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawItemSaveResult {
    pub inserted: i64,
    pub duplicated: i64,
    pub skipped: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_repo() -> RawItemRepository {
        let repo = RawItemRepository::open(Path::new(":memory:")).unwrap();
        repo.ensure_tables().unwrap();
        repo
    }

    #[test]
    fn test_save_and_dedup() {
        let repo = setup_repo();
        let items = vec![
            NewRawItem {
                source_url: "https://example.com".into(),
                item_type: "url".into(),
                item_hash: "abc123".into(),
                raw_content: None,
                extracted_url: Some("https://example.com/page1".into()),
            },
        ];
        let result = repo.save_items(&items).unwrap();
        assert_eq!(result.inserted, 1);
        assert_eq!(result.duplicated, 0);

        // Same hash -> duplicate
        let result = repo.save_items(&items).unwrap();
        assert_eq!(result.inserted, 0);
        assert_eq!(result.duplicated, 1);
    }

    #[test]
    fn test_pending_items_ordered_by_priority() {
        let repo = setup_repo();
        repo.save_items(&[
            NewRawItem {
                source_url: "a".into(), item_type: "url".into(),
                item_hash: "a1".into(), raw_content: None, extracted_url: None,
            },
            NewRawItem {
                source_url: "b".into(), item_type: "url".into(),
                item_hash: "b1".into(), raw_content: None,
                extracted_url: Some("https://b.com".into()),
            },
        ]).unwrap();

        let pending = repo.get_pending_items(10).unwrap();
        assert_eq!(pending.len(), 2);
        // Item with extracted_url should have higher priority
        assert_eq!(pending[0].extracted_url.as_deref(), Some("https://b.com"));
    }

    #[test]
    fn test_ignore_unmatched() {
        let repo = setup_repo();
        repo.save_items(&[
            NewRawItem {
                source_url: "a".into(), item_type: "url".into(),
                item_hash: "a1".into(), raw_content: None, extracted_url: None,
            },
        ]).unwrap();

        repo.assign_worker(1, "worker-1").unwrap();
        let ignored = repo.ignore_unmatched().unwrap();
        // Item 1 was matched, so no items should be ignored
        let ignored_count = repo.count_by_status("ignored").unwrap();
        assert_eq!(ignored_count, 0);
    }
}
