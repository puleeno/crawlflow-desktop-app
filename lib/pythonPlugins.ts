import { invoke } from '@tauri-apps/api/core';
import type { CrawlFlowPlugin, DataSourceDefinition, ProcessorDefinition, ParserDefinition } from '../types';
import { pluginManager } from './pluginManager';

interface PythonPluginMeta {
    id: string;
    name: string;
    version: string;
    description: string;
    capabilities: string[];
}

interface PipelineStep {
    processor_type: string;
    config: Record<string, any>;
}

/**
 * Bridge between Python plugins (running via PyO3 in Rust) and the frontend plugin system.
 *
 * Each discovered Python plugin with a "processor" capability gets registered
 * as a CrawlFlowPlugin so it appears in the Sidebar and Plugin Manager.
 */
class PythonPluginBridge {
    private loaded = false;

    async init(): Promise<void> {
        if (this.loaded) return;
        this.loaded = true;

        try {
            const metas: PythonPluginMeta[] = await invoke('list_python_plugins_cmd');
            for (const meta of metas) {
                this.registerPythonPlugin(meta);
            }
        } catch (e) {
            console.warn('Python plugins not available (running outside Tauri?):', e);
        }
    }

    private registerPythonPlugin(meta: PythonPluginMeta): void {
        const pluginId = `py-${meta.id}`;

        // Skip if already registered
        if (pluginManager.getPlugin(pluginId)) return;

        const plugin: CrawlFlowPlugin = {
            id: pluginId,
            name: `🐍 ${meta.name}`,
            version: meta.version,
            description: `${meta.description} (Python plugin)`,
            author: 'CrawlFlow (Python)',
            capabilities: [],
            configFields: [
                { key: '_python_config', label: 'Config (JSON)', type: 'textarea', placeholder: '{}' },
            ],
            defaultConfig: { _python_config: '{}' },
        };

        // Add data source capability
        if (meta.capabilities.includes('dataSource')) {
            plugin.capabilities.push('dataSource');
            plugin.dataSource = {
                type: `py-${meta.id}-source`,
                label: `${meta.name} Source`,
                description: meta.description,
                configFields: [
                    { key: '_plugin_id', label: 'Plugin', type: 'string', defaultValue: meta.id },
                    { key: '_python_config', label: 'Config (JSON)', type: 'textarea', placeholder: '{"source_type":"url","url":"https://..."}' },
                ],
                fetch: async (config) => {
                    const raw: any = await invoke('call_python_data_source_cmd', {
                        pluginId: meta.id,
                        config: { source_type: config.source_type || 'url', url: config.url, ...config },
                    });
                    return raw as any[];
                },
            };
        }

        // Add processor capability
        if (meta.capabilities.includes('processor')) {
            plugin.capabilities.push('processor');
            plugin.processor = {
                type: `py-${meta.id}`,
                label: meta.name,
                description: meta.description,
                configFields: [
                    { key: 'operation', label: 'Operation', type: 'select', defaultValue: 'passthrough',
                        options: [
                            { label: 'Passthrough', value: 'passthrough' },
                            { label: 'Select Fields', value: 'select_fields' },
                            { label: 'Rename Fields', value: 'rename_fields' },
                            { label: 'Add Field', value: 'add_field' },
                            { label: 'Filter', value: 'filter' },
                        ] },
                    { key: 'fields', label: 'Fields (comma-separated)', type: 'string', placeholder: 'title,body' },
                    { key: 'field', label: 'Field name', type: 'string', placeholder: 'my_field' },
                    { key: 'field_name', label: 'New field name', type: 'string', placeholder: 'new_field' },
                    { key: 'field_value', label: 'Default value', type: 'string', placeholder: 'default' },
                    { key: 'mapping', label: 'Rename mapping (JSON)', type: 'textarea', placeholder: '{"old_name":"new_name"}' },
                    { key: 'operator', label: 'Operator', type: 'select', defaultValue: 'equals',
                        options: [
                            { label: 'Equals', value: 'equals' },
                            { label: 'Contains', value: 'contains' },
                            { label: 'Greater Than', value: 'greater_than' },
                            { label: 'Less Than', value: 'less_than' },
                        ] },
                    { key: 'value', label: 'Value', type: 'string', placeholder: 'filter value' },
                ],
                process: async (data, config) => {
                    const result: any = await invoke('execute_python_hook_cmd', {
                        pluginId: meta.id,
                        hookName: 'process_data',
                        data,
                        config,
                    });
                    return result as any[];
                },
            };
        }

        // Add parser capability
        if (meta.capabilities.includes('parser')) {
            plugin.capabilities.push('parser');
            plugin.parser = {
                id: `py-${meta.id}-parser`,
                name: `${meta.name} Parser`,
                description: meta.description,
                inputFormats: ['html', 'xml', 'json'],
                configFields: [
                    { key: '_plugin_id', label: 'Plugin', type: 'string', defaultValue: meta.id },
                ],
                parse: async (input, _config) => {
                    const html = typeof input === 'string' ? input : String(input);
                    const config = { _plugin_id: meta.id };
                    const result: any = await invoke('execute_python_hook_cmd', {
                        pluginId: meta.id,
                        hookName: 'parse_data',
                        data: [],
                        config: { html, ...config },
                    });
                    return result as any[];
                },
            };
        }

        // Add export capability
        if (meta.capabilities.includes('export')) {
            plugin.capabilities.push('hook');
            const existingHooks = plugin.hooks || {};
            plugin.hooks = {
                ...existingHooks,
                customExport: async (ctx) => {
                    const output = await invoke('call_python_export_cmd', {
                        pluginId: meta.id,
                        data: ctx.crawlData || [],
                        config: ctx.config,
                    });
                    return { fileName: `${meta.id}_export`, mimeType: 'text/plain', content: output };
                },
            };
        }

        pluginManager.register(plugin);
    }

    /** Execute a pipeline of Python processor steps */
    async runPipeline(steps: PipelineStep[], initialData: any[]): Promise<any[]> {
        const result: any = await invoke('run_python_pipeline_cmd', { steps, initialData });
        return result as any[];
    }

    /** Reload Python plugins (e.g. after a new script is added) */
    async reload(): Promise<string[]> {
        this.loaded = false;
        const metas: PythonPluginMeta[] = await invoke('list_python_plugins_cmd');
        for (const meta of metas) {
            this.registerPythonPlugin(meta);
        }
        return metas.map(m => m.id);
    }
}

export const pythonPluginBridge = new PythonPluginBridge();
