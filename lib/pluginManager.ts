import type {
    CrawlFlowPlugin,
    PluginCapability,
    PluginHook,
    PluginHookContext,
    PluginRecord,
    DataSourceDefinition,
    ProcessorDefinition,
    ParserDefinition,
} from '../types';

class PluginManager {
    private registry: Map<string, CrawlFlowPlugin> = new Map();
    private enabledPlugins: Set<string> = new Set();
    private loaded = false;

    // Registered capabilities
    private dataSources: Map<string, DataSourceDefinition> = new Map();
    private processors: Map<string, ProcessorDefinition> = new Map();
    private parsers: Map<string, ParserDefinition> = new Map();

    async init() {
        if (this.loaded) return;
        this.loaded = true;

        const db = await this.getDb();
        if (!db) return;

        const rows: PluginRecord[] = await db.select('SELECT * FROM extensions WHERE enabled = 1');
        for (const row of rows) {
            this.enabledPlugins.add(row.id);
        }
    }

    private async getDb() {
        try {
            if (typeof window === 'undefined') return null;
            const tauri = !!(window as any).__TAURI_INTERNALS__?.ipc;
            if (!tauri) return null;
            const { default: Database } = await import('@tauri-apps/plugin-sql');
            return await Database.load('sqlite:crawlflow.db');
        } catch {
            return null;
        }
    }

    register(plugin: CrawlFlowPlugin) {
        this.registry.set(plugin.id, plugin);

        if (plugin.dataSource) {
            this.dataSources.set(plugin.dataSource.type, plugin.dataSource);
        }
        if (plugin.processor) {
            this.processors.set(plugin.processor.type, plugin.processor);
        }
        if (plugin.parser) {
            this.parsers.set(plugin.parser.id, plugin.parser);
        }
    }

    unregister(pluginId: string) {
        const plugin = this.registry.get(pluginId);
        if (plugin) {
            if (plugin.dataSource) this.dataSources.delete(plugin.dataSource.type);
            if (plugin.processor) this.processors.delete(plugin.processor.type);
            if (plugin.parser) this.parsers.delete(plugin.parser.id);
        }
        this.registry.delete(pluginId);
        this.enabledPlugins.delete(pluginId);
    }

    getPlugin(id: string): CrawlFlowPlugin | undefined {
        return this.registry.get(id);
    }

    getAllPlugins(): CrawlFlowPlugin[] {
        return Array.from(this.registry.values());
    }

    getEnabledPlugins(): CrawlFlowPlugin[] {
        return this.getAllPlugins().filter(p => this.enabledPlugins.has(p.id));
    }

    isEnabled(id: string): boolean {
        return this.enabledPlugins.has(id);
    }

    async setEnabled(id: string, enabled: boolean) {
        if (enabled) {
            this.enabledPlugins.add(id);
        } else {
            this.enabledPlugins.delete(id);
        }

        const db = await this.getDb();
        if (db) {
            if (enabled) {
                const plugin = this.registry.get(id);
                await db.execute(
                    `INSERT OR REPLACE INTO extensions (id, name, description, type, config, enabled)
                     VALUES ($1, $2, $3, 'plugin', '{}', 1)`,
                    [id, plugin?.name || id, plugin?.description || '']
                );
            } else {
                await db.execute('UPDATE extensions SET enabled = 0 WHERE id = $1', [id]);
            }
        }
    }

    // --- Data Source Capabilities ---

    getDataSources(): DataSourceDefinition[] {
        return Array.from(this.dataSources.values())
            .filter(ds => this.isEnabled(
                Array.from(this.registry.entries())
                    .find(([_, p]) => p.dataSource === ds)?.[0] || ''
            ));
    }

    getDataSource(type: string): DataSourceDefinition | undefined {
        return this.getDataSources().find(ds => ds.type === type);
    }

    async fetchDataSource(type: string, config: Record<string, any>): Promise<any[]> {
        const ds = this.getDataSource(type);
        if (!ds) throw new Error(`Data source "${type}" not found`);
        return ds.fetch(config);
    }

    // --- Processor Capabilities ---

    getProcessors(): ProcessorDefinition[] {
        return Array.from(this.processors.values())
            .filter(p => this.isEnabled(
                Array.from(this.registry.entries())
                    .find(([_, pl]) => pl.processor === p)?.[0] || ''
            ));
    }

    getProcessor(type: string): ProcessorDefinition | undefined {
        return this.getProcessors().find(p => p.type === type);
    }

    async processWithProcessor(type: string, data: any[], config: Record<string, any>): Promise<any[]> {
        const proc = this.getProcessor(type);
        if (!proc) throw new Error(`Processor "${type}" not found`);
        return proc.process(data, config);
    }

    // --- Parser Capabilities ---

    getParsers(): ParserDefinition[] {
        return Array.from(this.parsers.values())
            .filter(p => this.isEnabled(
                Array.from(this.registry.entries())
                    .find(([_, pl]) => pl.parser === p)?.[0] || ''
            ));
    }

    getParser(id: string): ParserDefinition | undefined {
        return this.getParsers().find(p => p.id === id);
    }

    getParsersForFormat(format: string): ParserDefinition[] {
        return this.getParsers().filter(p =>
            p.inputFormats.some(f => f.toLowerCase() === format.toLowerCase())
        );
    }

    async parseWithParser(id: string, input: string | any, config: Record<string, any>): Promise<any[]> {
        const parser = this.getParser(id);
        if (!parser) throw new Error(`Parser "${id}" not found`);
        return parser.parse(input, config);
    }

    // --- Hook Capabilities ---

    getPluginsForHook(hook: PluginHook): CrawlFlowPlugin[] {
        return this.getEnabledPlugins().filter(p => p.hooks?.[hook]);
    }

    async executeHook(hook: PluginHook, context: PluginHookContext): Promise<any> {
        const plugins = this.getPluginsForHook(hook);
        let result: any = context.crawlData;

        for (const plugin of plugins) {
            const handler = plugin.hooks![hook];
            if (handler) {
                try {
                    const pluginCtx = { ...context, config: { ...plugin.defaultConfig, ...context.config } };
                    result = await handler(pluginCtx);
                } catch (e) {
                    console.error(`Plugin "${plugin.name}" failed on hook "${hook}":`, e);
                }
            }
        }

        return result;
    }
}

export const pluginManager = new PluginManager();
