import { getPlatform, isTauri } from './platform';

export { isTauri };

const MASTER_SCHEMA_SQL = [
  `CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT DEFAULT '',
    db_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'disabled',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
  )`,
  `CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
  )`,
  `CREATE TABLE IF NOT EXISTS extensions (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT DEFAULT '',
    version TEXT DEFAULT '1.0.0',
    enabled INTEGER NOT NULL DEFAULT 1,
    installed_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
  )`,
  `CREATE TABLE IF NOT EXISTS project_runtime (
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
  )`,
  `CREATE TABLE IF NOT EXISTS raw_items (
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
  )`,
  `CREATE INDEX IF NOT EXISTS idx_raw_items_hash ON raw_items(item_hash)`,
  `CREATE INDEX IF NOT EXISTS idx_raw_items_status ON raw_items(status)`,
  `CREATE INDEX IF NOT EXISTS idx_raw_items_matched ON raw_items(matched)`,
  `CREATE INDEX IF NOT EXISTS idx_raw_items_worker ON raw_items(worker_id)`,
  `CREATE TABLE IF NOT EXISTS processing_log (
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
  )`,
  `CREATE INDEX IF NOT EXISTS idx_processing_log_item ON processing_log(item_id)`,
  `CREATE INDEX IF NOT EXISTS idx_processing_log_worker ON processing_log(worker_id)`,
  `CREATE TABLE IF NOT EXISTS parsed_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    raw_item_id INTEGER NOT NULL,
    worker_id TEXT NOT NULL,
    processor_id TEXT NOT NULL,
    data TEXT NOT NULL,
    schema_version TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (raw_item_id) REFERENCES raw_items(id),
    UNIQUE(id)
  )`,
  `CREATE INDEX IF NOT EXISTS idx_parsed_data_raw_item ON parsed_data(raw_item_id)`,
  `CREATE INDEX IF NOT EXISTS idx_parsed_data_worker ON parsed_data(worker_id)`,
  `CREATE INDEX IF NOT EXISTS idx_parsed_data_processor ON parsed_data(processor_id)`,
  `CREATE TABLE IF NOT EXISTS process_request_history (
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
  )`,
  `CREATE INDEX IF NOT EXISTS idx_process_request_history_worker ON process_request_history(worker_id)`,
  `CREATE INDEX IF NOT EXISTS idx_process_request_history_item ON process_request_history(raw_item_id)`,
  `CREATE INDEX IF NOT EXISTS idx_process_request_history_status ON process_request_history(status)`,
];

export async function ensureMasterDbSchema(db: any): Promise<void> {
  for (const sql of MASTER_SCHEMA_SQL) {
    await db.execute(sql);
  }
}

export async function getMasterDb(): Promise<any> {
  return getPlatform().getMasterDb();
}

export async function getProjectDb(projectId: string): Promise<any> {
  return getPlatform().getProjectDb(projectId);
}

export async function initProjectDb(db: any): Promise<void> {
  const migrations = [
    `CREATE TABLE IF NOT EXISTS project_settings (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    )`,
    `CREATE TABLE IF NOT EXISTS nodes (
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
    )`,
    `CREATE TABLE IF NOT EXISTS edges (
      id TEXT PRIMARY KEY,
      source TEXT NOT NULL,
      target TEXT NOT NULL,
      source_handle TEXT,
      target_handle TEXT,
      type TEXT DEFAULT 'smoothstep',
      animated INTEGER NOT NULL DEFAULT 0,
      data TEXT DEFAULT '{}',
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )`,
    `CREATE TABLE IF NOT EXISTS crawl_data (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      source_url TEXT,
      field_name TEXT NOT NULL,
      field_value TEXT,
      raw_data TEXT,
      node_id TEXT,
      extracted_at TEXT NOT NULL DEFAULT (datetime('now'))
    )`,
    `CREATE TABLE IF NOT EXISTS crawl_logs (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      level TEXT NOT NULL DEFAULT 'info',
      message TEXT NOT NULL,
      node_id TEXT,
      created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )`,
  ];

  for (const sql of migrations) {
    await db.execute(sql);
  }
}

export async function createProject(
  name: string,
  description: string = ''
): Promise<{ id: string; dbPath: string }> {
  const id = crypto.randomUUID();
  const dbPath = `project_${id}.db`;

  const master = await getMasterDb();
  await master.execute(
    `INSERT INTO projects (id, name, description, db_path) VALUES ($1, $2, $3, $4)`,
    [id, name, description, dbPath]
  );

  const projectDb = await getProjectDb(id);
  await initProjectDb(projectDb);

  await projectDb.execute(
    `INSERT INTO project_settings (key, value) VALUES ($1, $2), ($3, $4)`,
    ['name', name, 'description', description]
  );

  return { id, dbPath };
}

export async function createProjectFromPreset(
  presetName: string,
  presetDescription: string,
  settings: Record<string, any>,
  nodes: any[],
  edges: any[]
): Promise<{ id: string; dbPath: string }> {
  const id = crypto.randomUUID();
  const dbPath = `project_${id}.db`;

  const master = await getMasterDb();
  await master.execute(
    `INSERT INTO projects (id, name, description, db_path) VALUES ($1, $2, $3, $4)`,
    [id, presetName, presetDescription, dbPath]
  );

  const projectDb = await getProjectDb(id);
  await initProjectDb(projectDb);

  await saveProjectState(id, nodes, edges, settings);

  return { id, dbPath };
}

export async function listProjects(): Promise<any[]> {
  const db = await getMasterDb();
  return await db.select('SELECT * FROM projects ORDER BY updated_at DESC');
}

export async function deleteProject(id: string): Promise<void> {
  const platform = getPlatform();
  if (platform.name === 'tauri') {
    await platform.invoke('delete_project_cmd', { projectId: id });
  } else {
    const master = await getMasterDb();
    await master.execute('DELETE FROM projects WHERE id = $1', [id]);
  }
}

export async function saveProjectState(
  projectId: string,
  nodes: any[],
  edges: any[],
  settings: Record<string, any>
): Promise<void> {
  const db = await getProjectDb(projectId);

  await db.execute('DELETE FROM nodes');
  for (const node of nodes) {
    await db.execute(
      `INSERT OR REPLACE INTO nodes (id, type, label, position_x, position_y, data, deletable, draggable, width, height, z_index, parent_node)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)`,
      [
        node.id,
        node.type || '',
        node.data?.label || null,
        node.position.x,
        node.position.y,
        JSON.stringify(node.data || {}),
        node.deletable !== false ? 1 : 0,
        node.draggable !== false ? 1 : 0,
        node.width || null,
        node.height || null,
        node.zIndex || 0,
        node.parentNode || null,
      ]
    );
  }

  await db.execute('DELETE FROM edges');
  for (const edge of edges) {
    await db.execute(
      `INSERT OR REPLACE INTO edges (id, source, target, source_handle, target_handle, type, animated, data)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8)`,
      [
        edge.id,
        edge.source,
        edge.target,
        edge.sourceHandle || null,
        edge.targetHandle || null,
        edge.type || 'smoothstep',
        edge.animated ? 1 : 0,
        JSON.stringify(edge.data || {}),
      ]
    );
  }

  for (const [key, value] of Object.entries(settings)) {
    await db.execute(
      `INSERT OR REPLACE INTO project_settings (key, value) VALUES ($1, $2)`,
      [key, typeof value === 'string' ? value : JSON.stringify(value)]
    );
  }

  const status = (String(settings.enabled) === 'true' || settings.enabled === true) ? 'enabled' : 'disabled';
  const master = await getMasterDb();
  await master.execute(
    "UPDATE projects SET name = $1, description = $2, status = $3, updated_at = datetime('now') WHERE id = $4",
    [settings.name || 'Untitled', settings.description || '', status, projectId]
  );
}

export async function loadProjectState(projectId: string): Promise<{
  nodes: any[];
  edges: any[];
  settings: Record<string, string>;
}> {
  const db = await getProjectDb(projectId);

  const nodeRows: any[] = await db.select('SELECT * FROM nodes');
  const nodes = nodeRows.map((row: any) => ({
    id: row.id,
    type: row.type,
    position: { x: row.position_x, y: row.position_y },
    data: JSON.parse(row.data || '{}'),
    deletable: row.deletable === 1,
    draggable: row.draggable === 1,
    width: row.width,
    height: row.height,
    zIndex: row.z_index,
    parentNode: row.parent_node,
  }));

  const edgeRows: any[] = await db.select('SELECT * FROM edges');
  const edges = edgeRows.map((row: any) => ({
    id: row.id,
    source: row.source,
    target: row.target,
    sourceHandle: row.source_handle,
    targetHandle: row.target_handle,
    type: row.type,
    animated: row.animated === 1,
    data: JSON.parse(row.data || '{}'),
  }));

  const settingsRows: any[] = await db.select('SELECT * FROM project_settings');
  const settings: Record<string, string> = {};
  for (const row of settingsRows) {
    settings[row.key] = row.value;
  }

  return { nodes, edges, settings };
}
