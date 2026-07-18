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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlData {
    pub id: i64,
    pub raw_item_id: i64,
    pub content_type: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedData {
    pub id: i64,
    pub raw_item_id: i64,
    pub worker_id: Option<String>,
    pub processor_id: String,
    pub data: String,
    pub schema_version: i32,
    pub is_final: bool,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonLd {
    pub id: i64,
    pub raw_item_id: i64,
    pub ld_type: String,
    pub data: String,
    pub created_at: String,
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
        // Canonical schema. If an incompatible table already exists (e.g. one
        // created by an older build / the UI with a different `crawl_data`
        // layout), drop and recreate it so the whole schema stays consistent.
        let table_cols = |conn: &Connection, name: &str| -> Vec<String> {
            conn.query_row(
                &format!("SELECT sql FROM sqlite_master WHERE name = '{}'", name),
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .map(|sql| {
                sql.to_lowercase()
                    .split(',')
                    .map(|c| c.trim().split_whitespace().next().unwrap_or("").to_string())
                    .filter(|c| !c.is_empty())
                    .collect()
            })
            .unwrap_or_default()
        };

        // raw_items: drop if it lacks the canonical `source_url` column.
        if table_cols(&self.conn, "raw_items").contains(&"source_url".to_string()) == false
            && self.table_exists("raw_items")
        {
            self.conn.execute("DROP TABLE IF EXISTS raw_items", []).ok();
        }
        // crawl_data: drop if it lacks `raw_item_id` (old UI schema used source_url).
        if table_cols(&self.conn, "crawl_data").contains(&"raw_item_id".to_string()) == false
            && self.table_exists("crawl_data")
        {
            self.conn
                .execute("DROP TABLE IF EXISTS crawl_data", [])
                .ok();
        }

        let statements = [
            "CREATE TABLE IF NOT EXISTS raw_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_url TEXT NOT NULL,
                item_type TEXT NOT NULL DEFAULT 'url',
                item_hash TEXT NOT NULL,
                dup_count INTEGER NOT NULL DEFAULT 1,
                priority INTEGER NOT NULL DEFAULT 0,
                worker_id TEXT,
                matched INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            "CREATE INDEX IF NOT EXISTS idx_raw_items_hash ON raw_items(item_hash)",
            "CREATE INDEX IF NOT EXISTS idx_raw_items_status ON raw_items(status)",
            "CREATE INDEX IF NOT EXISTS idx_raw_items_matched ON raw_items(matched)",
            "CREATE INDEX IF NOT EXISTS idx_raw_items_worker ON raw_items(worker_id)",
            "CREATE TABLE IF NOT EXISTS crawl_data (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                raw_item_id INTEGER NOT NULL,
                content_type TEXT NOT NULL DEFAULT 'raw',
                content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            "CREATE INDEX IF NOT EXISTS idx_crawl_data_item ON crawl_data(raw_item_id)",
            "CREATE TABLE IF NOT EXISTS parsed_data (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                raw_item_id INTEGER NOT NULL,
                worker_id TEXT,
                processor_id TEXT NOT NULL DEFAULT '',
                data TEXT NOT NULL DEFAULT '{}',
                schema_version INTEGER NOT NULL DEFAULT 1,
                is_final INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'pending',
                error TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            "CREATE INDEX IF NOT EXISTS idx_parsed_data_item ON parsed_data(raw_item_id)",
            "CREATE INDEX IF NOT EXISTS idx_parsed_data_final ON parsed_data(raw_item_id, is_final)",
            "CREATE TABLE IF NOT EXISTS json_ld (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                raw_item_id INTEGER NOT NULL,
                ld_type TEXT NOT NULL DEFAULT 'unknown',
                data TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            "CREATE INDEX IF NOT EXISTS idx_json_ld_item ON json_ld(raw_item_id)",
        ];
        for stmt in statements {
            if let Err(e) = self.conn.execute(stmt, []) {
                if !e.to_string().contains("already exists") {
                    return Err(format!("Failed to create table ({}): {}", stmt, e));
                }
            }
        }
        Ok(())
    }

    fn table_exists(&self, name: &str) -> bool {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = ?1",
                params![name],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0
    }

    /// Save items with dedup. Increments dup_count for existing items.
    /// Returns the new row IDs (in insertion order) so callers can persist
    /// fetched content into `crawl_data` keyed by raw_item_id.
    pub fn save_items(&self, items: &[NewRawItem]) -> Result<RawItemSaveResult, String> {
        let mut inserted = 0i64;
        let mut duplicated = 0i64;
        let mut ids: Vec<i64> = Vec::new();

        for item in items {
            // Check existing by hash
            let existing: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM raw_items WHERE item_hash = ?1",
                    params![item.item_hash],
                    |row| row.get(0),
                )
                .ok();

            match existing {
                Some(id) => {
                    // Increment dup_count and recalculate priority
                    self.conn
                        .execute(
                            "UPDATE raw_items SET dup_count = dup_count + 1,
                         priority = dup_count + 1,
                         updated_at = datetime('now')
                         WHERE id = ?1",
                            params![id],
                        )
                        .map_err(|e| format!("Failed to update dup_count: {}", e))?;
                    duplicated += 1;
                }
                None => {
                    let priority = 5;
                    self.conn
                        .execute(
                            "INSERT INTO raw_items (source_url, item_type, item_hash, priority)
                         VALUES (?1, ?2, ?3, ?4)",
                            params![item.source_url, item.item_type, item.item_hash, priority,],
                        )
                        .map_err(|e| format!("Failed to insert item: {}", e))?;
                    let new_id = self.conn.last_insert_rowid();
                    ids.push(new_id);
                    inserted += 1;
                }
            }
        }

        Ok(RawItemSaveResult {
            inserted,
            duplicated,
            ids,
        })
    }

    /// Persist fetched raw content into `crawl_data`, keyed by raw_item_id.
    pub fn save_crawl_data(
        &self,
        raw_item_id: i64,
        content_type: &str,
        content: &str,
    ) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO crawl_data (raw_item_id, content_type, content) VALUES (?1, ?2, ?3)",
                params![raw_item_id, content_type, content],
            )
            .map_err(|e| format!("Failed to save crawl_data: {}", e))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Read crawled content by raw_item_id (latest first).
    pub fn get_crawl_data(&self, raw_item_id: i64) -> Result<Vec<CrawlData>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, raw_item_id, content_type, content, created_at
                 FROM crawl_data WHERE raw_item_id = ?1 ORDER BY id DESC",
            )
            .map_err(|e| format!("Failed to prepare crawl_data query: {}", e))?;
        let rows = stmt
            .query_map(params![raw_item_id], |row| {
                Ok(CrawlData {
                    id: row.get(0)?,
                    raw_item_id: row.get(1)?,
                    content_type: row.get(2).unwrap_or_default(),
                    content: row.get(3).unwrap_or_default(),
                    created_at: row.get(4).unwrap_or_default(),
                })
            })
            .map_err(|e| format!("Failed to query crawl_data: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Lấy nội dung thô (HTML/data) mới nhất của một raw_item từ `crawl_data`.
    /// Trả `None` nếu chưa có. Tiện cho worker đọc lại content để parse.
    pub fn get_crawl_data_content(&self, raw_item_id: i64) -> Option<String> {
        self.get_crawl_data(raw_item_id)
            .ok()
            .and_then(|mut v| v.pop())
            .map(|c| c.content)
    }

    /// Look up a raw_item id by its hash (used to attach crawl_data after insert).
    pub fn get_raw_item_id_by_hash(&self, item_hash: &str) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT id FROM raw_items WHERE item_hash = ?1 LIMIT 1",
                params![item_hash],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to lookup raw_item by hash: {}", e))
    }

    /// Lấy tất cả JSON-LD blocks của một raw_item.
    pub fn get_json_ld(&self, raw_item_id: i64) -> Result<Vec<JsonLd>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, raw_item_id, ld_type, data, created_at
                 FROM json_ld WHERE raw_item_id = ?1 ORDER BY id",
            )
            .map_err(|e| format!("Failed to prepare json_ld query: {}", e))?;
        let rows = stmt
            .query_map(params![raw_item_id], |row| {
                Ok(JsonLd {
                    id: row.get(0)?,
                    raw_item_id: row.get(1)?,
                    ld_type: row.get(2)?,
                    data: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| format!("Failed to query json_ld: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Persist a JSON-LD block auto-extracted from crawled HTML.
    pub fn save_json_ld(&self, raw_item_id: i64, ld_type: &str, data: &str) -> Result<i64, String> {
        self.conn
            .execute(
                "INSERT INTO json_ld (raw_item_id, ld_type, data) VALUES (?1, ?2, ?3)",
                params![raw_item_id, ld_type, data],
            )
            .map_err(|e| format!("Failed to save json_ld: {}", e))?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Persist one processor-chain step result into `parsed_data`.
    /// `is_final = true` marks the terminal processor's output (used by export).
    pub fn save_parsed_data(
        &self,
        raw_item_id: i64,
        worker_id: Option<&str>,
        processor_id: &str,
        data: &str,
        is_final: bool,
    ) -> Result<i64, String> {
        self.conn.execute(
            "INSERT INTO parsed_data (raw_item_id, worker_id, processor_id, data, is_final, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'done')",
            params![
                raw_item_id,
                worker_id,
                processor_id,
                data,
                if is_final { 1i64 } else { 0i64 },
            ],
        ).map_err(|e| format!("Failed to save parsed_data: {}", e))?;
        let id = self.conn.last_insert_rowid();
        Ok(id)
    }

    /// Read all `crawl_data` rows (source_url + raw JSON payload) so the
    /// export processor can build its input from the structured product data
    /// the plugins saved, independent of the `parsed_data` table.
    pub fn get_all_crawl_data_json(&self) -> Result<Vec<(i64, String)>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT raw_item_id, content FROM crawl_data
                 WHERE content IS NOT NULL AND content <> ''
                 ORDER BY id DESC",
            )
            .map_err(|e| format!("Failed to prepare crawl_data query: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("Failed to query crawl_data: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Read the final parsed result for a raw_item (is_final = 1).
    pub fn get_final_parsed(&self, raw_item_id: i64) -> Result<Option<ParsedData>, String> {
        let row: Option<ParsedData> = self
            .conn
            .query_row(
                "SELECT id, raw_item_id, worker_id, processor_id, data, schema_version, is_final, status, error, created_at
                 FROM parsed_data WHERE raw_item_id = ?1 AND is_final = 1
                 ORDER BY id DESC LIMIT 1",
                params![raw_item_id],
                |r| {
                    Ok(ParsedData {
                        id: r.get(0)?,
                        raw_item_id: r.get(1)?,
                        worker_id: r.get(2)?,
                        processor_id: r.get(3)?,
                        data: r.get(4)?,
                        schema_version: r.get(5)?,
                        is_final: r.get(6)?,
                        status: r.get(7)?,
                        error: r.get(8)?,
                        created_at: r.get(9)?,
                    })
                },
            )
            .ok();
        Ok(row)
    }

    /// Save raw HTML fetched from a data source (debug / audit trail).
    /// Inserts with item_type='raw', status='crawled' so it is skipped by `get_pending_items`.
    /// Save a fetched raw source: metadata row in `raw_items` (item_type
    /// reflects the source kind: data_source / rss / csv / xml / json / url)
    /// plus its content in `crawl_data` (content_type='raw'), plus any
    /// JSON-LD blocks auto-extracted from the HTML into `json_ld`.
    pub fn save_raw_source(
        &self,
        source_url: &str,
        item_type: &str,
        raw_html: &str,
    ) -> Result<i64, String> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let hash_input = format!("{}:{}", source_url, raw_html);
        hash_input.hash(&mut hasher);
        let item_hash = format!("{:x}", hasher.finish());

        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM raw_items WHERE item_hash = ?1",
                params![item_hash],
                |row| row.get(0),
            )
            .ok();

        let raw_item_id = if let Some(id) = existing {
            id
        } else {
            self.conn
                .execute(
                    "INSERT INTO raw_items (source_url, item_type, item_hash, status, priority)
                     VALUES (?1, ?2, ?3, 'crawled', 1)",
                    params![source_url, item_type, item_hash],
                )
                .map_err(|e| format!("Failed to save raw source: {}", e))?;
            self.conn.last_insert_rowid()
        };

        // Persist fetched content separately in crawl_data.
        let _ = self.save_crawl_data(raw_item_id, "raw", raw_html);

        // Auto-extract JSON-LD blocks from the HTML into json_ld.
        let json_lds = crate::crawler::extract_json_ld_blocks(raw_html);
        for ld in &json_lds {
            let ld_type = ld
                .get("@type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let ld_json = serde_json::to_string(ld).unwrap_or_default();
            let _ = self.save_json_ld(raw_item_id, &ld_type, &ld_json);
        }
        if !json_lds.is_empty() {
            log::info!(
                "[repository] Extracted {} JSON-LD block(s) from {}",
                json_lds.len(),
                source_url
            );
        }

        Ok(raw_item_id)
    }

    /// Lấy các items pending (chưa xử lý)
    pub fn get_pending_items(&self, limit: i64) -> Result<Vec<RawItem>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_url, item_type, item_hash,
                     dup_count, priority, worker_id, matched, status, created_at, updated_at
              FROM raw_items
              WHERE status = 'pending' AND matched = 0
              ORDER BY priority DESC, dup_count DESC
              LIMIT ?1",
            )
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let items = stmt
            .query_map(params![limit], |row| {
                Ok(RawItem {
                    id: row.get(0)?,
                    source_url: row.get(1)?,
                    item_type: row.get(2)?,
                    item_hash: row.get(3)?,
                    dup_count: row.get(4)?,
                    priority: row.get(5)?,
                    worker_id: row.get(6)?,
                    matched: row.get(7)?,
                    status: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })
            .map_err(|e| format!("Failed to query items: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    /// Lấy items đã match với worker
    pub fn get_matched_items(&self, worker_id: &str, limit: i64) -> Result<Vec<RawItem>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_url, item_type, item_hash,
                     dup_count, priority, worker_id, matched, status, created_at, updated_at
              FROM raw_items
              WHERE worker_id = ?1 AND status = 'pending' AND matched = 1
              ORDER BY priority DESC
              LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let items = stmt
            .query_map(params![worker_id, limit], |row| {
                Ok(RawItem {
                    id: row.get(0)?,
                    source_url: row.get(1)?,
                    item_type: row.get(2)?,
                    item_hash: row.get(3)?,
                    dup_count: row.get(4)?,
                    priority: row.get(5)?,
                    worker_id: row.get(6)?,
                    matched: row.get(7)?,
                    status: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })
            .map_err(|e| format!("Failed to query items: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    /// Alias for get_matched_items, used by the pipeline processing phase
    pub fn get_matched_items_for_worker(
        &self,
        worker_id: &str,
        limit: i64,
    ) -> Result<Vec<RawItem>, String> {
        self.get_matched_items(worker_id, limit)
    }

    /// Gán worker cho item
    pub fn assign_worker(&self, item_id: i64, worker_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE raw_items SET worker_id = ?1, matched = 1, updated_at = datetime('now')
             WHERE id = ?2",
                params![worker_id, item_id],
            )
            .map_err(|e| format!("Failed to assign worker: {}", e))?;
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
        self.conn
            .execute(
                "UPDATE raw_items SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
                params![status, item_id],
            )
            .map_err(|e| format!("Failed to update status: {}", e))?;
        Ok(())
    }

    /// Log một bước xử lý của processor.
    /// Thay thế bảng `processing_log` cũ: kết quả trung gian/final được lưu vào
    /// `parsed_data`. `is_final=true` khi `processor_type=="final_output"`.
    pub fn log_processing(
        &self,
        item_id: i64,
        worker_id: Option<&str>,
        processor_type: &str,
        _status: &str,
        output: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), String> {
        let is_final = processor_type == "final_output";
        let parsed_json = if let Some(err) = error {
            serde_json::json!({ "output": output.unwrap_or(""), "error": err }).to_string()
        } else {
            output.unwrap_or("").to_string()
        };
        match self.save_parsed_data(item_id, worker_id, processor_type, &parsed_json, is_final) {
            Ok(_) => Ok(()),
            Err(e) => {
                eprintln!(
                    "[log_processing] save_parsed_data failed for item {}: {}",
                    item_id, e
                );
                Err(e)
            }
        }
    }

    /// Dem so items theo status
    pub fn count_by_status(&self, status: &str) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM raw_items WHERE status = ?1",
                params![status],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to count items: {}", e))
    }

    /// Reset items pending lai sau khi xử lý lỗi
    #[allow(dead_code)]
    pub fn reset_failed_items(&self) -> Result<i64, String> {
        let count = self
            .conn
            .execute(
                "UPDATE raw_items SET status = 'pending', updated_at = datetime('now')
             WHERE status = 'error'",
                [],
            )
            .map_err(|e| format!("Failed to reset failed items: {}", e))?;
        Ok(count as i64)
    }

    /// Reset items bị kẹt ở 'processing' về 'pending' (recovery sau crash/restart).
    /// Items bị matched=1 sẽ giữ nguyên worker_id để được pick up ngay ở Phase 3.
    pub fn reset_stale_processing_items(&self) -> Result<i64, String> {
        let count = self
            .conn
            .execute(
                "UPDATE raw_items SET status = 'pending', updated_at = datetime('now')
             WHERE status = 'processing'",
                [],
            )
            .map_err(|e| format!("Failed to reset stale processing items: {}", e))?;
        Ok(count as i64)
    }

    /// Đếm số items có status='done' và item_type='url'
    pub fn count_done_url_items(&self) -> Result<i64, String> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM raw_items WHERE status='done' AND item_type='url'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to count done url items: {}", e))
    }

    /// Reset 'done' URL items về 'pending' + unmatched để pipeline re-fetch và re-parse.
    /// Dùng khi extract_rules thay đổi hoặc cần crawl lại chi tiết trang.
    pub fn reset_done_url_items_to_pending(&self) -> Result<i64, String> {
        let count = self
            .conn
            .execute(
                "UPDATE raw_items SET status = 'pending', matched = 0, worker_id = NULL,
                 updated_at = datetime('now')
                 WHERE status = 'done' AND item_type = 'url'",
                [],
            )
            .map_err(|e| format!("Failed to reset done url items: {}", e))?;
        Ok(count as i64)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawItemSaveResult {
    pub inserted: i64,
    pub duplicated: i64,
    /// IDs of newly-inserted raw_items (in insertion order), so callers
    /// can persist fetched content into `crawl_data` keyed by id.
    pub ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemsSummary {
    pub total: i64,
    pub pending: i64,
    pub processing: i64,
    pub done: i64,
    pub error: i64,
    pub ignored: i64,
    pub crawled: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemsQuery {
    pub status: Option<String>,
    pub worker_id: Option<String>,
    pub search: Option<String>,
    pub matched: Option<i32>,
    pub limit: i64,
    pub offset: i64,
    pub sort_by: Option<String>,
    pub sort_dir: Option<String>,
}

pub struct PaginatedItems {
    pub items: Vec<RawItem>,
    pub total: i64,
}

impl RawItemRepository {
    /// Query items with filter by status, search text, and pagination
    pub fn query_items(&self, query: &ItemsQuery) -> Result<PaginatedItems, String> {
        let sort_by = query.sort_by.as_deref().unwrap_or("created_at");
        let sort_dir = query.sort_dir.as_deref().unwrap_or("DESC");
        let order_clause = format!("ORDER BY {} {}", sort_by, sort_dir);

        // Build WHERE clause and params as string values
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<String> = Vec::new();

        if let Some(ref status) = query.status {
            clauses.push(format!("status = ?{}", params.len() + 1));
            params.push(status.clone());
        }
        if let Some(ref worker_id) = query.worker_id {
            clauses.push(format!("worker_id = ?{}", params.len() + 1));
            params.push(worker_id.clone());
        }
        if let Some(matched) = query.matched {
            clauses.push(format!("matched = ?{}", params.len() + 1));
            params.push(matched.to_string());
        }
        if let Some(ref search) = query.search {
            let pattern = format!("%{}%", search);
            let n = params.len();
            clauses.push(format!("(source_url LIKE ?{})", n + 1));
            params.push(pattern.clone());
            params.push(pattern);
        }

        let where_clause = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        // Count
        let count_sql = format!("SELECT COUNT(*) FROM raw_items {}", where_clause);
        let total: i64 = self.query_row_str(&count_sql, &params)?;

        // Query with limit/offset appended
        params.push(query.limit.to_string());
        params.push(query.offset.to_string());
        let query_sql = format!(
            "SELECT id, source_url, item_type, item_hash,
                    dup_count, priority, worker_id, matched, status, created_at, updated_at
             FROM raw_items {} {} LIMIT ?{} OFFSET ?{}",
            where_clause,
            order_clause,
            params.len() - 1,
            params.len()
        );

        let items = self.query_items_raw(&query_sql, &params)?;
        Ok(PaginatedItems {
            items: items.items,
            total,
        })
    }

    fn query_row_str(&self, sql: &str, params: &[String]) -> Result<i64, String> {
        let p: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        self.conn
            .query_row(sql, p.as_slice(), |r| r.get(0))
            .map_err(|e| format!("Query failed: {}", e))
    }

    fn query_items_raw(&self, sql: &str, params: &[String]) -> Result<PaginatedItems, String> {
        let p: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(|e| format!("Prepare failed: {}", e))?;
        let items = stmt
            .query_map(p.as_slice(), |row| {
                Ok(RawItem {
                    id: row.get(0)?,
                    source_url: row.get(1)?,
                    item_type: row.get(2)?,
                    item_hash: row.get(3)?,
                    dup_count: row.get(4)?,
                    priority: row.get(5)?,
                    worker_id: row.get(6)?,
                    matched: row.get(7)?,
                    status: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })
            .map_err(|e| format!("Query failed: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(PaginatedItems { items, total: 0 })
    }

    /// Count items by all statuses
    pub fn get_summary(&self) -> Result<ItemsSummary, String> {
        let total = self
            .conn
            .query_row("SELECT COUNT(*) FROM raw_items", [], |r| r.get(0))
            .map_err(|e| format!("Failed to count: {}", e))?;
        let pending = self.count_by_status("pending")?;
        let processing = self.count_by_status("processing")?;
        let done = self.count_by_status("done")?;
        let error = self.count_by_status("error")?;
        let ignored = self.count_by_status("ignored")?;
        let crawled = self.count_by_status("crawled")?;
        Ok(ItemsSummary {
            total,
            pending,
            processing,
            done,
            error,
            ignored,
            crawled,
        })
    }

    /// Get done items (processed successfully) với kết quả final từ `parsed_data`
    /// (is_final=1), thay thế việc đọc từ `processing_log` cũ.
    pub fn get_done_items(&self, limit: i64) -> Result<Vec<(RawItem, Option<String>)>, String> {
        let mut stmt = self.conn.prepare(
            "SELECT r.id, r.source_url, r.item_type, r.item_hash,
                    r.dup_count, r.priority, r.worker_id, r.matched, r.status, r.created_at, r.updated_at,
                    p.data
             FROM raw_items r
             LEFT JOIN (
                 SELECT raw_item_id, data, MAX(id) AS max_id
                 FROM parsed_data
                 WHERE is_final = 1
                 GROUP BY raw_item_id
             ) p ON p.raw_item_id = r.id
             WHERE r.status = 'done'
             ORDER BY r.updated_at DESC
             LIMIT ?1"
        ).map_err(|e| format!("Failed to prepare query: {}", e))?;

        let items = stmt
            .query_map(params![limit], |row| {
                Ok((
                    RawItem {
                        id: row.get(0)?,
                        source_url: row.get(1)?,
                        item_type: row.get(2)?,
                        item_hash: row.get(3)?,
                        dup_count: row.get(4)?,
                        priority: row.get(5)?,
                        worker_id: row.get(6)?,
                        matched: row.get(7)?,
                        status: row.get(8)?,
                        created_at: row.get(9)?,
                        updated_at: row.get(10)?,
                    },
                    row.get::<_, Option<String>>(11).ok().flatten(),
                ))
            })
            .map_err(|e| format!("Failed to query done items: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }

    /// Get all crawled raw items (with status='crawled' and item_type='raw')
    pub fn get_crawled_items(&self) -> Result<Vec<RawItem>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_url, item_type, item_hash,
                     dup_count, priority, worker_id, matched, status, created_at, updated_at
              FROM raw_items
              WHERE status = 'crawled'
              ORDER BY created_at ASC",
            )
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let items = stmt
            .query_map([], |row| {
                Ok(RawItem {
                    id: row.get(0)?,
                    source_url: row.get(1)?,
                    item_type: row.get(2)?,
                    item_hash: row.get(3)?,
                    dup_count: row.get(4)?,
                    priority: row.get(5)?,
                    worker_id: row.get(6)?,
                    matched: row.get(7)?,
                    status: row.get(8)?,
                    created_at: row.get(9)?,
                    updated_at: row.get(10)?,
                })
            })
            .map_err(|e| format!("Failed to query items: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(items)
    }
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
        let items = vec![NewRawItem {
            source_url: "https://example.com".into(),
            item_type: "url".into(),
            item_hash: "abc123".into(),
        }];
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
                source_url: "a".into(),
                item_type: "url".into(),
                item_hash: "a1".into(),
            },
            NewRawItem {
                source_url: "b".into(),
                item_type: "url".into(),
                item_hash: "b1".into(),
            },
        ])
        .unwrap();

        let pending = repo.get_pending_items(10).unwrap();
        assert_eq!(pending.len(), 2);
    }

    #[test]
    fn test_ignore_unmatched() {
        let repo = setup_repo();
        repo.save_items(&[NewRawItem {
            source_url: "a".into(),
            item_type: "url".into(),
            item_hash: "a1".into(),
        }])
        .unwrap();

        repo.assign_worker(1, "worker-1").unwrap();
        let _ignored = repo.ignore_unmatched().unwrap();
        // Item 1 was matched, so no items should be ignored
        let ignored_count = repo.count_by_status("ignored").unwrap();
        assert_eq!(ignored_count, 0);
    }

    #[test]
    fn test_raw_source_persists_crawl_data_and_json_ld() {
        let repo = setup_repo();
        let html = r#"<html><head>
            <script type="application/ld+json">{"@type":"Product","name":"Test"}</script>
        </head><body>hello</body></html>"#;
        let raw_id = repo
            .save_raw_source("https://example.com/s", "raw", html)
            .unwrap();
        assert!(raw_id > 0);

        // crawl_data should hold the raw HTML
        let content = repo.get_crawl_data_content(raw_id);
        assert_eq!(content.as_deref(), Some(html));

        // json_ld should have auto-extracted the Product block
        let lds = repo.get_json_ld(raw_id).unwrap();
        assert_eq!(lds.len(), 1);
        assert_eq!(lds[0].ld_type, "Product");
    }

    #[test]
    fn test_parsed_data_final_flag() {
        let repo = setup_repo();
        let item = NewRawItem {
            source_url: "https://x.com".into(),
            item_type: "url".into(),
            item_hash: "h1".into(),
        };
        let saved = repo.save_items(&[item]).unwrap();
        let raw_id = saved.ids[0];

        repo.save_parsed_data(raw_id, Some("w1"), "proc-1", r#"{"a":1}"#, false)
            .unwrap();
        repo.save_parsed_data(raw_id, Some("w1"), "final_output", r#"{"a":1,"b":2}"#, true)
            .unwrap();

        let final_parsed = repo.get_final_parsed(raw_id).unwrap();
        assert!(final_parsed.is_some());
        assert_eq!(final_parsed.unwrap().processor_id, "final_output");
    }
}
