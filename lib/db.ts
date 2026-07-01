let tauriAvailable = false;

export function isTauri(): boolean {
    try { return typeof window !== 'undefined' && !!(window as any).__TAURI_INTERNALS__?.ipc; }
    catch { return false; }
}

export function setTauriAvailable(val: boolean) {
    tauriAvailable = val;
}

export async function getMasterDb(): Promise<any> {
    if (!isTauri()) throw new Error('Not in Tauri environment');

    const { default: Database } = await import('@tauri-apps/plugin-sql');
    return await Database.load('sqlite:crawlflow.db');
}

export async function getProjectDb(projectId: string): Promise<any> {
    if (!isTauri()) throw new Error('Not in Tauri environment');

    const { default: Database } = await import('@tauri-apps/plugin-sql');
    return await Database.load(`sqlite:project_${projectId}.db`);
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
        `INSERT INTO project_settings (key, value) VALUES ('name', $1), ('description', $2)`,
        [name, description]
    );

    return { id, dbPath };
}

export async function listProjects(): Promise<any[]> {
    const db = await getMasterDb();
    return await db.select('SELECT * FROM projects ORDER BY updated_at DESC');
}

export async function deleteProject(id: string): Promise<void> {
    const master = await getMasterDb();
    await master.execute('DELETE FROM projects WHERE id = $1', [id]);
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
