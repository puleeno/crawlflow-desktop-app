type SqlRow = Record<string, unknown>;

export interface Database {
  execute(sql: string, bindings?: unknown[]): Promise<{ rowsAffected: number; lastInsertId?: number }>;
  select<T = SqlRow>(sql: string, bindings?: unknown[]): Promise<T[]>;
}

export interface PlatformAdapter {
  readonly name: 'tauri' | 'browser';
  isTauri(): boolean;
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  getMasterDb(): Promise<Database>;
  getProjectDb(projectId: string): Promise<Database>;
}

// ── Tauri implementation ────────────────────────────────────────────────

class TauriPlatform implements PlatformAdapter {
  readonly name = 'tauri' as const;

  isTauri(): boolean {
    return true;
  }

  async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }

  async getMasterDb(): Promise<Database> {
    const { default: Database } = await import('@tauri-apps/plugin-sql');
    const db = await Database.load('sqlite:crawlflow.db');
    await this._ensureMasterSchema(db);
    return db;
  }

  async getProjectDb(projectId: string): Promise<Database> {
    const { default: Database } = await import('@tauri-apps/plugin-sql');
    return await Database.load(`sqlite:project_${projectId}.db`);
  }

  private async _ensureMasterSchema(db: Database): Promise<void> {
    const sql = [
      `CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT DEFAULT '', db_path TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'disabled', created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now')))`,
      `CREATE TABLE IF NOT EXISTS app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)`,
      `CREATE TABLE IF NOT EXISTS extensions (id TEXT PRIMARY KEY, type TEXT NOT NULL, name TEXT NOT NULL, description TEXT DEFAULT '', version TEXT DEFAULT '1.0.0', enabled INTEGER NOT NULL DEFAULT 1, installed_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now')))`,
      `CREATE TABLE IF NOT EXISTS project_runtime (project_id TEXT PRIMARY KEY, runner_status TEXT NOT NULL DEFAULT 'stopped', runner_pid INTEGER, runner_type TEXT DEFAULT 'service', service_control TEXT NOT NULL DEFAULT 'stopped', edit_pid INTEGER, cycle_count INTEGER NOT NULL DEFAULT 0, last_run_at TEXT, last_error TEXT, updated_at TEXT NOT NULL DEFAULT (datetime('now')))`,
    ];
    for (const s of sql) {
      await db.execute(s);
    }
  }
}

// ── Browser implementation ──────────────────────────────────────────────

type Table = SqlRow[];
type BrowserDb = Map<string, Table>;
type InvokeHandler = (args: Record<string, unknown>) => unknown;

class BrowserPlatform implements PlatformAdapter {
  readonly name = 'browser' as const;

  private _masterDb: InMemoryDatabase | null = null;
  private _projectDbs = new Map<string, InMemoryDatabase>();
  private _nextId = 1;
  private _invokeMocks = new Map<string, InvokeHandler>();

  constructor() {
    this._registerDefaultMocks();
  }

  private _registerDefaultMocks(): void {
    this._invokeMocks.set('delete_project_cmd', (args) => {
      const db = this._getMaster();
      db.execute(`DELETE FROM projects WHERE id = $1`, [args.projectId]);
      return null;
    });
    this._invokeMocks.set('get_service_status_cmd', (args) => this._mockServiceStatus(args?.projectId as string));
    this._invokeMocks.set('list_project_services_cmd', () => this._mockListServices());
    this._invokeMocks.set('request_project_run_cmd', (args) => {
      this._mockSetServiceControl(args?.projectId as string, 'run');
      return null;
    });
    this._invokeMocks.set('request_project_stop_cmd', (args) => {
      this._mockSetServiceControl(args?.projectId as string, 'stop');
      return null;
    });
    this._invokeMocks.set('list_project_services_cmd', () => this._mockListServices());
    this._invokeMocks.set('get_app_setting_cmd', () => null);
    this._invokeMocks.set('set_app_setting_cmd', () => null);
    this._invokeMocks.set('detect_python_cmd', () => null);
    this._invokeMocks.set('get_service_install_info_cmd', () => ({
      installed: false,
      running: false,
    }));
    this._invokeMocks.set('install_system_service_cmd', () => 'ok (mock)');
    this._invokeMocks.set('uninstall_system_service_cmd', () => 'ok (mock)');
    this._invokeMocks.set('start_system_service_cmd', () => 'ok (mock)');
    this._invokeMocks.set('stop_system_service_cmd', () => 'ok (mock)');
    this._invokeMocks.set('lock_project_edit_cmd', () => null);
    this._invokeMocks.set('unlock_project_edit_cmd', () => null);
    this._invokeMocks.set('request_project_run_cmd', (args) => {
      this._mockSetServiceControl(args?.projectId as string, 'run');
      return null;
    });
    this._invokeMocks.set('request_project_stop_cmd', (args) => {
      this._mockSetServiceControl(args?.projectId as string, 'stop');
      return null;
    });
    this._invokeMocks.set('clear_project_logs_cmd', () => null);
    this._invokeMocks.set('list_presets_cmd', () => []);
    this._invokeMocks.set('list_python_plugins_cmd', () => []);
    this._invokeMocks.set('call_python_data_source_cmd', () => ({}));
    this._invokeMocks.set('execute_python_hook_cmd', () => ({ output: '{}' }));
    this._invokeMocks.set('call_python_filter_cmd', () => ({ output: '{}' }));
    this._invokeMocks.set('call_python_export_cmd', () => ({ output: '' }));
    this._invokeMocks.set('run_python_pipeline_cmd', () => ({ output: '[]' }));
    this._invokeMocks.set('parse_html_with_bs4_cmd', () => ({ output: '[]' }));
    this._invokeMocks.set('summarize_parsed_html_cmd', () => ({ output: '' }));
    this._invokeMocks.set('fetch_url_cmd', () => ({ html: '', status: 200 }));
    this._invokeMocks.set('export_csv_cmd', () => null);
    this._invokeMocks.set('export_excel_cmd', () => null);
    this._invokeMocks.set('execute_processor_cmd', () => ({ output: '{}' }));
    this._invokeMocks.set('fetch_rss_cmd', () => []);
    this._invokeMocks.set('parse_html_table_cmd', () => []);
    this._invokeMocks.set('run_demo_cmd', () => null);
    this._invokeMocks.set('install_marketplace_item', () => 'ok (mock)');
  }

  isTauri(): boolean {
    return false;
  }

  async invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    const handler = this._invokeMocks.get(cmd);
    if (handler) {
      return handler(args ?? {}) as T;
    }
    console.warn(`[BrowserPlatform] No mock for invoke('${cmd}')`, args);
    return undefined as T;
  }

  private _mockProgressCounters = new Map<string, { cycle: number; items: number; fails: number; msgIdx: number }>();
  private _mockMsgs = [
    'Fetching product URLs…',
    'Parsing product pages…',
    'Extracting metadata…',
    'Processing images…',
    'Running data plugins…',
    'Generating export file…',
    'Finalizing…',
  ];

  /** Simulate service status for a single project. */
  private _mockServiceStatus(projectId: string): Record<string, unknown> {
    if (!projectId) return this._mkServiceInfo('', 'stopped');
    const table = (this._getMaster() as any)._tables.get('project_runtime') || [];
    const row = table.find((r: any) => r.project_id === projectId);
    const status = row?.runner_status === 'running' ? 'running' : 'stopped';
    return this._mkServiceInfo(projectId, status);
  }

  /** Simulate service status for all projects. */
  private _mockListServices(): Record<string, unknown>[] {
    const table = (this._getMaster() as any)._tables.get('project_runtime') || [];
    return table.map((row: any) =>
      this._mkServiceInfo(row.project_id, row.runner_status === 'running' ? 'running' : 'stopped'),
    );
  }

  /** Set service_control for a project so the mock shows it as "running". */
  private _mockSetServiceControl(projectId: string, control: string): void {
    if (!projectId) return;
    const db = this._getMaster();
    db.execute(
      `INSERT OR REPLACE INTO project_runtime (project_id, runner_status, service_control, cycle_count, updated_at) VALUES ($1, $2, $3, $4, datetime('now'))`,
      [projectId, control === 'run' ? 'running' : 'stopped', control, 0],
    );
    if (control === 'run') {
      this._mockProgressCounters.set(projectId, { cycle: 0, items: 0, fails: 0, msgIdx: 0 });
    } else {
      this._mockProgressCounters.delete(projectId);
    }
  }

  private _mkServiceInfo(projectId: string, status: string): Record<string, unknown> {
    let progress: Record<string, unknown> | null = null;
    if (status === 'running') {
      let counter = this._mockProgressCounters.get(projectId);
      if (!counter) {
        counter = { cycle: 0, items: 0, fails: 0, msgIdx: 0 };
        this._mockProgressCounters.set(projectId, counter);
      }
      // Advance simulated progress each time this is polled
      counter.items = Math.min(counter.items + Math.floor(Math.random() * 7) + 1, 200);
      counter.fails += Math.random() < 0.1 ? 1 : 0;
      if (counter.items >= 200) {
        counter.cycle++;
        counter.items = 0;
        counter.fails = 0;
      }
      counter.msgIdx = Math.min(counter.msgIdx + (Math.random() < 0.25 ? 1 : 0), this._mockMsgs.length - 1);
      if (counter.items === 0) counter.msgIdx = 0;

      const total = 200;
      const done = counter.items;
      const pending = Math.max(0, total - done);
      const pct = (done / total) * 100;

      progress = {
        progress_pct: pct,
        message: this._mockMsgs[counter.msgIdx],
        items_total: total,
        items_processed: done,
        items_success: Math.max(0, done - counter.fails),
        items_failed: counter.fails,
        items_pending: pending,
        current_url: '',
        started_at: new Date(Date.now() - 60000).toISOString(),
        eta_secs: Math.max(0, Math.floor((total - done) * 0.3)),
      };
    }
    return {
      project_id: projectId,
      status,
      runner_pid: status === 'running' ? 99999 : null,
      cycle_count: 0,
      started_at: status === 'running' ? new Date().toISOString() : '',
      last_run_at: '',
      last_error: null,
      interval_seconds: 60,
      progress,
      ws_port: 0,
    };
  }

  private _getMaster(): InMemoryDatabase {
    if (!this._masterDb) {
      this._masterDb = new InMemoryDatabase('master', this);
      this._ensureMasterSchema(this._masterDb);
    }
    return this._masterDb;
  }

  private _ensureMasterSchema(db: InMemoryDatabase): void {
    db.execute(`CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT DEFAULT '', db_path TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'disabled', created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now')))`);
    db.execute(`CREATE TABLE IF NOT EXISTS app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)`);
    db.execute(`CREATE TABLE IF NOT EXISTS extensions (id TEXT PRIMARY KEY, type TEXT NOT NULL, name TEXT NOT NULL, description TEXT DEFAULT '', version TEXT DEFAULT '1.0.0', enabled INTEGER NOT NULL DEFAULT 1, installed_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now')))`);
    db.execute(`CREATE TABLE IF NOT EXISTS project_runtime (project_id TEXT PRIMARY KEY, runner_status TEXT NOT NULL DEFAULT 'stopped', runner_pid INTEGER, runner_type TEXT DEFAULT 'service', service_control TEXT NOT NULL DEFAULT 'stopped', edit_pid INTEGER, cycle_count INTEGER NOT NULL DEFAULT 0, last_run_at TEXT, last_error TEXT, updated_at TEXT NOT NULL DEFAULT (datetime('now')))`);
    db.execute(`CREATE TABLE IF NOT EXISTS raw_items (id INTEGER PRIMARY KEY AUTOINCREMENT, source_url TEXT NOT NULL, item_type TEXT NOT NULL DEFAULT 'url', item_hash TEXT NOT NULL, raw_content TEXT, extracted_url TEXT, dup_count INTEGER NOT NULL DEFAULT 1, priority INTEGER NOT NULL DEFAULT 0, worker_id TEXT, matched INTEGER NOT NULL DEFAULT 0, status TEXT NOT NULL DEFAULT 'pending', created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now')))`);
    db.execute(`CREATE TABLE IF NOT EXISTS processing_log (id INTEGER PRIMARY KEY AUTOINCREMENT, item_id INTEGER NOT NULL, worker_id TEXT, processor_type TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending', output TEXT, error TEXT, started_at TEXT, finished_at TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')))`);
    db.execute(`CREATE TABLE IF NOT EXISTS parsed_data (id INTEGER PRIMARY KEY AUTOINCREMENT, raw_item_id INTEGER NOT NULL, worker_id TEXT NOT NULL, processor_id TEXT NOT NULL, data TEXT NOT NULL, schema_version TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')))`);
    db.execute(`CREATE TABLE IF NOT EXISTS process_request_history (id INTEGER PRIMARY KEY AUTOINCREMENT, worker_id TEXT NOT NULL, raw_item_id INTEGER NOT NULL, processor_id TEXT NOT NULL, processor_type TEXT NOT NULL, input_data TEXT, input_config TEXT, output_data TEXT, retry_count INTEGER DEFAULT 0, max_retry INTEGER DEFAULT 3, status TEXT NOT NULL DEFAULT 'pending', error_message TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')), finished_at TEXT)`);
  }

  async getMasterDb(): Promise<Database> {
    return this._getMaster();
  }

  async getProjectDb(projectId: string): Promise<Database> {
    if (!this._projectDbs.has(projectId)) {
      const db = new InMemoryDatabase(`project_${projectId}`, this);
      this._initProjectSchema(db);
      this._projectDbs.set(projectId, db);
    }
    return this._projectDbs.get(projectId)!;
  }

  private _initProjectSchema(db: InMemoryDatabase): void {
    db.execute(`CREATE TABLE IF NOT EXISTS project_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)`);
    db.execute(`CREATE TABLE IF NOT EXISTS nodes (id TEXT PRIMARY KEY, type TEXT NOT NULL, label TEXT, position_x REAL NOT NULL DEFAULT 0, position_y REAL NOT NULL DEFAULT 0, data TEXT NOT NULL DEFAULT '{}', deletable INTEGER NOT NULL DEFAULT 1, draggable INTEGER NOT NULL DEFAULT 1, width REAL, height REAL, z_index INTEGER DEFAULT 0, parent_node TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now')))`);
    db.execute(`CREATE TABLE IF NOT EXISTS edges (id TEXT PRIMARY KEY, source TEXT NOT NULL, target TEXT NOT NULL, source_handle TEXT, target_handle TEXT, type TEXT DEFAULT 'smoothstep', animated INTEGER NOT NULL DEFAULT 0, data TEXT DEFAULT '{}', created_at TEXT NOT NULL DEFAULT (datetime('now')))`);
    db.execute(`CREATE TABLE IF NOT EXISTS crawl_data (id INTEGER PRIMARY KEY AUTOINCREMENT, source_url TEXT, field_name TEXT NOT NULL, field_value TEXT, raw_data TEXT, node_id TEXT, extracted_at TEXT NOT NULL DEFAULT (datetime('now')))`);
    db.execute(`CREATE TABLE IF NOT EXISTS crawl_logs (id INTEGER PRIMARY KEY AUTOINCREMENT, level TEXT NOT NULL DEFAULT 'info', message TEXT NOT NULL, node_id TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')))`);
  }

  /** exposed for InMemoryDatabase auto-increment */
  nextId(): number {
    return this._nextId++;
  }
}

// ── In-memory SQL-like database for browser mode ───────────────────────

class InMemoryDatabase implements Database {
  private _tables = new Map<string, Table>();
  private _defaults = new Map<string, Record<string, unknown>>();
  private _name: string;
  private _platform: BrowserPlatform;

  constructor(name: string, platform: BrowserPlatform) {
    this._name = name;
    this._platform = platform;
  }

  async execute(sql: string, bindings?: unknown[]): Promise<{ rowsAffected: number; lastInsertId?: number }> {
    console.debug(`[InMemoryDB:${this._name}] execute:`, sql, bindings);
    const trimmed = sql.trim().toUpperCase();

    if (trimmed.startsWith('CREATE TABLE')) {
      this._execCreateTable(sql);
      return { rowsAffected: 0 };
    }

    if (trimmed.startsWith('CREATE INDEX')) {
      return { rowsAffected: 0 };
    }

    if (trimmed.startsWith('INSERT')) {
      return this._execInsert(sql, bindings);
    }

    if (trimmed.startsWith('UPDATE')) {
      return this._execUpdate(sql, bindings);
    }

    if (trimmed.startsWith('DELETE')) {
      return this._execDelete(sql, bindings);
    }

    return { rowsAffected: 0 };
  }

  private _execCreateTable(sql: string): void {
    const nameMatch = sql.match(/CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)/i);
    if (!nameMatch) return;
    const name = nameMatch[1];
    if (this._tables.has(name)) return;
    this._tables.set(name, []);

    const cols = this._parseColumns(sql);
    this._defaults.set(name, cols);
  }

  private _parseColumns(sql: string): Record<string, unknown> {
    const parenMatch = sql.match(/\((.+)\)\s*$/s);
    if (!parenMatch) return {};
    const body = parenMatch[1];
    const defaults: Record<string, unknown> = {};
    for (const line of body.split(',')) {
      const part = line.trim();
      if (/^(PRIMARY|FOREIGN|UNIQUE|INDEX|CONSTRAINT)/i.test(part)) continue;
      const colMatch = part.match(/^(\w+)\s+/);
      if (!colMatch) continue;
      const col = colMatch[1];
      const defMatch = part.match(/DEFAULT\s+(\S+)/i);
      if (defMatch) {
        let val: unknown = defMatch[1];
        if (val === 'NULL') val = null;
        else if (/^\d+(\.\d+)?$/.test(val as string)) val = Number(val);
        else if ((val as string).startsWith("'") || (val as string).startsWith('"')) val = (val as string).slice(1, -1);
        defaults[col] = val;
      }
      // sqlite auto-increment only for INTEGER PRIMARY KEY
      const isPk = /\bPRIMARY\s+KEY\b/i.test(part);
      const isInt = /\bINTEGER\b/i.test(part);
      if (isPk && isInt) {
        // auto-increment id — no default, will be generated
      }
    }
    return defaults;
  }

  async select<T = SqlRow>(sql: string, bindings?: unknown[]): Promise<T[]> {
    console.debug(`[InMemoryDB:${this._name}] select:`, sql, bindings);
    const trimmed = sql.trim().toUpperCase();

    if (!trimmed.startsWith('SELECT')) return [];

    const tableMatch = sql.match(/FROM\s+(\w+)/i);
    if (!tableMatch) return [];
    const tableName = tableMatch[1];
    const table = this._tables.get(tableName);
    if (!table) return [];

    let rows = [...table];

    const whereClause = this._extractWhere(sql);
    if (whereClause) {
      rows = rows.filter((row) => this._evalWhere(row, whereClause, bindings ?? []));
    }

    const orderMatch = sql.match(/ORDER\s+BY\s+(.+?)(?:\s+(ASC|DESC))?(?:\s|$)/i);
    if (orderMatch) {
      const col = orderMatch[1].trim();
      const desc = orderMatch[2]?.toUpperCase() === 'DESC';
      rows.sort((a, b) => {
        const va = a[col] ?? '';
        const vb = b[col] ?? '';
        if (typeof va === 'number' && typeof vb === 'number') return desc ? vb - va : va - vb;
        return desc ? String(vb).localeCompare(String(va)) : String(va).localeCompare(String(vb));
      });
    }

    const selectMatch = sql.match(/SELECT\s+(.+?)\s+FROM/i);
    if (!selectMatch) return rows as T[];
    const selectCols = selectMatch[1].trim();

    if (selectCols === '*') return rows as T[];

    const cols = selectCols.split(',').map((c) => c.trim());
    return rows.map((row) => {
      const result: SqlRow = {};
      for (const col of cols) {
        const asMatch = col.match(/(\w+)\s+AS\s+(\w+)/i);
        if (asMatch) {
          result[asMatch[2]] = row[asMatch[1]];
        } else {
          result[col] = row[col];
        }
      }
      return result as T;
    });
  }

  private _execInsert(sql: string, bindings?: unknown[]): { rowsAffected: number; lastInsertId?: number } {
    const tableMatch = sql.match(/INSERT\s+(?:OR\s+REPLACE\s+)?INTO\s+(\w+)/i);
    if (!tableMatch) return { rowsAffected: 0 };
    const tableName = tableMatch[1];
    const table = this._tables.get(tableName);
    if (!table) return { rowsAffected: 0 };

    const colMatch = sql.match(/\((.+?)\)\s*(?:VALUES|SELECT)/i);
    const valMatch = sql.match(/VALUES\s*\((.+?)\)/i);
    if (!colMatch || !valMatch) return { rowsAffected: 0 };

    const cols = colMatch[1].split(',').map((c) => c.trim().replace(/['"`]/g, ''));
    const placeholders = valMatch[1].split(',').map((p) => p.trim());

    const isReplace = /INSERT\s+OR\s+REPLACE/i.test(sql);

    if (placeholders.length === 1 && placeholders[0].startsWith('SELECT')) {
      return { rowsAffected: 0 };
    }

    const row: SqlRow = {};
    let lastInsertId: number | undefined;

    let idIdx = cols.indexOf('id');
    for (let i = 0; i < cols.length; i++) {
      const val = placeholders[i].startsWith('$') || placeholders[i].startsWith('?')
        ? bindings?.[parseInt(placeholders[i].slice(1)) - 1]
        : placeholders[i].replace(/^['"]|['"]$/g, '');

      if (cols[i].toLowerCase() === 'id' && (val === undefined || val === null)) {
        const id = this._platform.nextId();
        row[cols[i]] = id;
        lastInsertId = id;
      } else if (cols[i].toLowerCase() === 'id') {
        row[cols[i]] = val;
        lastInsertId = val as number;
      } else {
        row[cols[i]] = val;
      }
    }

    if (idIdx === -1) {
      row['id'] = this._platform.nextId();
    }

    // Apply DEFAULT values for any columns not set in the INSERT
    const tableDefaults = this._defaults.get(tableName);
    if (tableDefaults) {
      for (const [col, val] of Object.entries(tableDefaults)) {
        if (row[col] === undefined) {
          row[col] = val;
        }
      }
    }

    if (isReplace) {
      const pkMatch = sql.match(/INSERT\s+OR\s+REPLACE\s+INTO\s+(\w+)\s*\((.+?)\)/i);
      if (pkMatch) {
        const pkCols = pkMatch[2].split(',').map((c) => c.trim().replace(/['"`]/g, ''));
        const pkCol = pkCols[0];
        const existingIdx = table.findIndex((r) => r[pkCol] === row[pkCol]);
        if (existingIdx >= 0) {
          table[existingIdx] = row;
          return { rowsAffected: 1 };
        }
      }
    }

    table.push(row);
    return { rowsAffected: 1, lastInsertId };
  }

  private _execUpdate(sql: string, bindings?: unknown[]): { rowsAffected: number } {
    const tableMatch = sql.match(/UPDATE\s+(\w+)/i);
    if (!tableMatch) return { rowsAffected: 0 };
    const tableName = tableMatch[1];
    const table = this._tables.get(tableName);
    if (!table) return { rowsAffected: 0 };

    const setMatch = sql.match(/SET\s+(.+?)(?:\s+WHERE|$)/i);
    if (!setMatch) return { rowsAffected: 0 };

    const setClauses = setMatch[1].split(',').map((s) => s.trim());
    const whereClause = this._extractWhere(sql);
    let rowsAffected = 0;

    for (const row of table) {
      if (whereClause && !this._evalWhere(row, whereClause, bindings ?? [])) continue;
      for (const clause of setClauses) {
        const eqMatch = clause.match(/(\w+)\s*=\s*(.+)/i);
        if (!eqMatch) continue;
        const col = eqMatch[1].trim();
        const valExpr = eqMatch[2].trim();
        if (valExpr.startsWith('$') || valExpr.startsWith('?')) {
          const idx = parseInt(valExpr.slice(1)) - 1;
          row[col] = bindings?.[idx];
        } else if (/^\w+\(/i.test(valExpr)) {
          // function like datetime('now')
          if (/datetime\s*\(\s*'now'\s*\)/i.test(valExpr)) {
            row[col] = new Date().toISOString();
          } else {
            row[col] = valExpr;
          }
        } else {
          row[col] = valExpr.replace(/^['"]|['"]$/g, '');
        }
      }
      rowsAffected++;
    }

    return { rowsAffected };
  }

  private _execDelete(sql: string, bindings?: unknown[]): { rowsAffected: number } {
    const tableMatch = sql.match(/DELETE\s+FROM\s+(\w+)/i);
    if (!tableMatch) return { rowsAffected: 0 };
    const tableName = tableMatch[1];
    const table = this._tables.get(tableName);
    if (!table) return { rowsAffected: 0 };

    const whereClause = this._extractWhere(sql);
    if (!whereClause) {
      const count = table.length;
      this._tables.set(tableName, []);
      return { rowsAffected: count };
    }

    const remaining = table.filter((row) => !this._evalWhere(row, whereClause, bindings ?? []));
    const rowsAffected = table.length - remaining.length;
    this._tables.set(tableName, remaining);
    return { rowsAffected };
  }

  private _extractWhere(sql: string): string | null {
    const match = sql.match(/WHERE\s+(.+?)(?:\s+ORDER\s+BY|$)/i);
    return match ? match[1].trim() : null;
  }

  private _evalWhere(row: SqlRow, whereClause: string, bindings: unknown[]): boolean {
    const conditions = whereClause.split(/\s+AND\s+/i);
    for (const cond of conditions) {
      const parts = cond.match(/(\w+)\s*(=|!=|<>|>=|<=|>|<|IS\s+NOT|IS)\s*(.+)/i);
      if (!parts) continue;
      const col = parts[1].trim();
      let op = parts[2].trim().toUpperCase();
      let valExpr = parts[3].trim();

      if (op === 'IS NOT') op = '!=';
      else if (op === 'IS') op = '==';

      let val: unknown;
      if (valExpr.toUpperCase() === 'NULL') {
        val = null;
      } else if (valExpr.startsWith('$') || valExpr.startsWith('?')) {
        const idx = parseInt(valExpr.slice(1)) - 1;
        val = bindings[idx];
      } else if (/^\d+(\.\d+)?$/.test(valExpr)) {
        val = Number(valExpr);
      } else if (valExpr.startsWith("'") || valExpr.startsWith('"')) {
        val = valExpr.slice(1, -1);
      } else if (/^\w+\(/.test(valExpr)) {
        val = valExpr;
      } else {
        val = valExpr;
      }

      const rowVal = row[col];

      if (op === '=' || op === '==') {
        if (rowVal !== val) return false;
      } else if (op === '!=' || op === '<>') {
        if (rowVal == val) return false;
      } else if (op === '>') {
        if (!(rowVal as number) > (val as number)) return false;
      } else if (op === '<') {
        if (!(rowVal as number) < (val as number)) return false;
      } else if (op === '>=') {
        if (!(rowVal as number) >= (val as number)) return false;
      } else if (op === '<=') {
        if (!(rowVal as number) <= (val as number)) return false;
      }
    }
    return true;
  }
}

// ── Singleton ───────────────────────────────────────────────────────────

let _platform: PlatformAdapter | null = null;

export function getPlatform(): PlatformAdapter {
  if (!_platform) {
    if (typeof window !== 'undefined' && ((window as any).__TAURI_INTERNALS__ || (window as any).__TAURI__)) {
      _platform = new TauriPlatform();
    } else {
      _platform = new BrowserPlatform();
    }
  }
  return _platform;
}

/** True when running inside a Tauri WebView. */
export function isTauri(): boolean {
  return getPlatform().isTauri();
}

/** Call a Tauri command (mocked in browser mode). */
export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getPlatform().invoke<T>(cmd, args);
}

/** Subscribe to a Tauri event (simulated via timer in browser mode). */
const _browserListeners = new Map<string, Set<(event: { payload: any }) => void>>();
let _browserTimer: ReturnType<typeof setInterval> | null = null;

function _startBrowserEventSimulator(): void {
  if (_browserTimer) return;
  _browserTimer = setInterval(() => {
    const platform = getPlatform();
    if (platform.name !== 'browser') return;
    const bp = platform as BrowserPlatform;

    // Emit service-status-update for every "running" project
    const handlerSet = _browserListeners.get('service-status-update');
    if (handlerSet && handlerSet.size > 0) {
      const infos = (bp as any)._mockListServices();
      for (const info of infos) {
        if (info.status === 'running') {
          const payload = { project_id: info.project_id, info };
          for (const h of handlerSet) {
            h({ payload });
          }
        }
      }
    }
  }, 2000);
}

export async function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  if (isTauri()) {
    const { listen: tauriListen } = await import('@tauri-apps/api/event');
    return tauriListen(event, handler);
  }
  // Browser mode – register simulated listener
  if (!_browserListeners.has(event)) {
    _browserListeners.set(event, new Set());
  }
  _browserListeners.get(event)!.add(handler as any);
  _startBrowserEventSimulator();
  return () => {
    _browserListeners.get(event)?.delete(handler as any);
  };
}

/** Open a file/directory picker (returns null in browser mode). */
export async function openDialog(options?: Record<string, unknown>): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  return (open as any)(options) as Promise<string | null>;
}

/** Show a save dialog. */
export async function saveDialog(options?: Record<string, unknown>): Promise<string | null> {
  if (!isTauri()) return null;
  const { save } = await import('@tauri-apps/plugin-dialog');
  return (save as any)(options) as Promise<string | null>;
}

/** Show a message dialog. */
export async function messageDialog(msg: string, options?: Record<string, unknown>): Promise<void> {
  if (!isTauri()) return;
  const { message } = await import('@tauri-apps/plugin-dialog');
  return (message as any)(msg, options);
}

/** Show a confirm dialog. */
export async function askDialog(message: string, options?: Record<string, unknown>): Promise<boolean> {
  if (!isTauri()) return false;
  const { ask } = await import('@tauri-apps/plugin-dialog');
  return (ask as any)(message, options) as Promise<boolean>;
}

/** Read a text file (returns null in browser mode). */
export async function readTextFile(path: string): Promise<string | null> {
  if (!isTauri()) return null;
  const { readTextFile: read } = await import('@tauri-apps/plugin-fs');
  return (read as any)(path) as Promise<string>;
}

/** Write a text file (no-op in browser mode). */
export async function writeTextFile(path: string, contents: string): Promise<void> {
  if (!isTauri()) return;
  const { writeTextFile: write } = await import('@tauri-apps/plugin-fs');
  return (write as any)(path, contents);
}
