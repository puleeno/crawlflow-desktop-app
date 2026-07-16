-- Migration v3: Add parsed_data and process_request_history tables

CREATE TABLE IF NOT EXISTS parsed_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    raw_item_id INTEGER NOT NULL,
    worker_id TEXT NOT NULL,
    processor_id TEXT NOT NULL,
    data TEXT NOT NULL,
    schema_version TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (raw_item_id) REFERENCES raw_items(id),
    UNIQUE(id)
);

CREATE INDEX IF NOT EXISTS idx_parsed_data_raw_item ON parsed_data(raw_item_id);
CREATE INDEX IF NOT EXISTS idx_parsed_data_worker ON parsed_data(worker_id);
CREATE INDEX IF NOT EXISTS idx_parsed_data_processor ON parsed_data(processor_id);

-- Migration to add process_request_history table for tracking processor chain execution

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
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at TEXT,
    FOREIGN KEY (raw_item_id) REFERENCES raw_items(id),
    FOREIGN KEY (worker_id) REFERENCES workers(id)
);

CREATE INDEX IF NOT EXISTS idx_process_request_history_worker ON process_request_history(worker_id);
CREATE INDEX IF NOT EXISTS idx_process_request_history_item ON process_request_history(raw_item_id);
CREATE INDEX IF NOT EXISTS idx_process_request_history_status ON process_request_history(status);