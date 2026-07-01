import { invoke } from '@tauri-apps/api/core';
import type { CrawlFlowPlugin } from '../../types';

// =====================================================
// CSV Export Plugin - execution via Rust
// =====================================================
export const csvExportPlugin: CrawlFlowPlugin = {
    id: 'csv-export',
    name: 'CSV Export',
    version: '1.0.0',
    description: 'Export crawled data to CSV format (Rust backend).',
    author: 'CrawlFlow',
    capabilities: ['hook'],
    hooks: {
        customExport: async (ctx) => {
            const result: any = await invoke('export_csv_cmd', {
                request: {
                    format: 'csv',
                    data: ctx.crawlData || [],
                    config: ctx.config,
                },
            });
            // Trigger download
            const blob = new Blob([result.content], { type: result.mime_type });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = result.file_name;
            document.body.appendChild(a);
            a.click();
            document.body.removeChild(a);
            URL.revokeObjectURL(url);
            return result;
        },
    },
    configFields: [
        { key: 'delimiter', label: 'Delimiter', type: 'select', defaultValue: ',',
            options: [{ label: 'Comma (,)', value: ',' }, { label: 'Semicolon (;)', value: ';' }] },
        { key: 'includeHeader', label: 'Include Header Row', type: 'boolean', defaultValue: true },
    ],
    defaultConfig: { delimiter: ',', includeHeader: true },
};

// =====================================================
// JSON Transform Plugin - Rust-side processor
// =====================================================
export const jsonTransformPlugin: CrawlFlowPlugin = {
    id: 'json-transform',
    name: 'JSON Transform',
    version: '1.0.0',
    description: 'Transform crawled data via Rust processors.',
    author: 'CrawlFlow',
    capabilities: ['hook'],
    hooks: {
        transformData: async (ctx) => {
            // Forward to Rust processor pipeline
            const pipelines = (ctx.config.pipeline as any[]) || [];
            if (pipelines.length === 0) return ctx.crawlData;

            let data = ctx.crawlData || [];
            for (const step of pipelines) {
                const result: any = await invoke('execute_processor_cmd', {
                    request: {
                        processor_type: step.id,
                        data,
                        config: step.config || {},
                    },
                });
                if (result.success) {
                    data = result.data;
                }
            }
            return data;
        },
    },
    configFields: [{
        key: 'pipeline',
        label: 'Processor Pipeline Config (JSON)',
        type: 'textarea',
        placeholder: '[{"id":"rust-deduplicate","config":{"field":"id"}}]',
    }],
    defaultConfig: { pipeline: [] },
};

// =====================================================
// Send to API Plugin - uses Rust HTTP client
// =====================================================
export const apiExportPlugin: CrawlFlowPlugin = {
    id: 'api-export',
    name: 'Send to API',
    version: '1.0.0',
    description: 'Send crawled data to REST API via Rust HTTP client.',
    author: 'CrawlFlow',
    capabilities: ['hook'],
    hooks: {
        beforeSave: async (ctx) => {
            const endpoint = ctx.config.endpointUrl as string;
            if (!endpoint) return ctx.crawlData;

            try {
                await fetch(endpoint, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(ctx.crawlData),
                });
            } catch (e) {
                console.error('API export failed:', e);
            }
            return ctx.crawlData;
        },
    },
    configFields: [
        { key: 'endpointUrl', label: 'API Endpoint URL', type: 'string', required: true, placeholder: 'https://api.example.com/data' },
    ],
    defaultConfig: { endpointUrl: '' },
};

// =====================================================
// RSS Feed Data Source - execution via Rust
// =====================================================
export const rssDataSourcePlugin: CrawlFlowPlugin = {
    id: 'rss-feed-source',
    name: 'RSS Feed Source',
    version: '1.0.0',
    description: 'Fetch data from RSS/Atom feeds via Rust backend.',
    author: 'CrawlFlow',
    capabilities: ['dataSource'],
    dataSource: {
        type: 'rss-feed',
        label: 'RSS Feed',
        description: 'Import data from RSS or Atom feeds',
        configFields: [
            { key: 'feedUrl', label: 'Feed URL', type: 'string', required: true, placeholder: 'https://example.com/feed.xml' },
            { key: 'maxItems', label: 'Max Items', type: 'number', defaultValue: 50 },
        ],
        fetch: async (config) => {
            const result: any[] = await invoke('fetch_rss_cmd', {
                request: {
                    feed_url: config.feedUrl,
                    max_items: parseInt(config.maxItems as string) || 50,
                },
            });
            return result;
        },
    },
};

// =====================================================
// Data Aggregator Processor - uses Rust processor
// =====================================================
export const dataAggregatorPlugin: CrawlFlowPlugin = {
    id: 'data-aggregator',
    name: 'Data Aggregator',
    version: '1.0.0',
    description: 'Aggregate and summarize data via Rust backend.',
    author: 'CrawlFlow',
    capabilities: ['processor'],
    processor: {
        type: 'data-aggregator',
        label: 'Data Aggregator',
        description: 'Aggregate data with group-by, count, sum, average',
        configFields: [
            { key: 'groupBy', label: 'Group By Field', type: 'string', placeholder: 'category' },
            { key: 'operation', label: 'Operation', type: 'select', defaultValue: 'count',
                options: [{ label: 'Count', value: 'count' }, { label: 'Sum', value: 'sum' }, { label: 'Average', value: 'avg' }] },
            { key: 'valueField', label: 'Value Field (for sum/avg)', type: 'string', placeholder: 'price' },
            { key: 'outputField', label: 'Output Field Name', type: 'string', defaultValue: 'result' },
        ],
        process: async (data, config) => {
            const result: any = await invoke('execute_processor_cmd', {
                request: {
                    processor_type: 'rust-deduplicate',
                    data,
                    config,
                },
            });
            return result.success ? result.data : data;
        },
    },
};

// =====================================================
// HTML Table Parser - execution via Rust
// =====================================================
export const htmlTableParserPlugin: CrawlFlowPlugin = {
    id: 'html-table-parser',
    name: 'HTML Table Parser',
    version: '1.0.0',
    description: 'Parse HTML tables via Rust backend (scraper crate).',
    author: 'CrawlFlow',
    capabilities: ['parser'],
    parser: {
        id: 'html-table',
        name: 'HTML Table',
        description: 'Extract data from HTML <table> elements',
        inputFormats: ['html', 'htm'],
        configFields: [
            { key: 'tableIndex', label: 'Table Index (0-based)', type: 'number', defaultValue: 0 },
            { key: 'hasHeader', label: 'First row is header', type: 'boolean', defaultValue: true },
        ],
        parse: async (input, config) => {
            const html = typeof input === 'string' ? input : String(input);
            const result: any[] = await invoke('parse_html_table_cmd', {
                html,
                config: {
                    tableIndex: parseInt(config.tableIndex as string) || 0,
                    hasHeader: config.hasHeader !== false,
                },
            });
            return result;
        },
    },
};

// =====================================================
// Rust Processor wrappers (use Rust processors from frontend)
// =====================================================
export const deduplicatePlugin: CrawlFlowPlugin = {
    id: 'rust-deduplicate-ui',
    name: 'Deduplicate (Rust)',
    version: '1.0.0',
    description: 'Remove duplicate items based on a field (Rust backend).',
    author: 'CrawlFlow',
    capabilities: ['processor'],
    processor: {
        type: 'rust-deduplicate',
        label: 'Deduplicate',
        description: 'Remove duplicates by field',
        configFields: [
            { key: 'field', label: 'Field to check', type: 'string', required: true, defaultValue: 'id' },
        ],
        process: async (data, config) => {
            const result: any = await invoke('execute_processor_cmd', {
                request: { processor_type: 'rust-deduplicate', data, config },
            });
            return result.success ? result.data : data;
        },
    },
};

export const filterPlugin: CrawlFlowPlugin = {
    id: 'rust-filter-ui',
    name: 'Filter (Rust)',
    version: '1.0.0',
    description: 'Filter data by field conditions (Rust backend).',
    author: 'CrawlFlow',
    capabilities: ['processor'],
    processor: {
        type: 'rust-filter',
        label: 'Filter',
        description: 'Filter rows by condition',
        configFields: [
            { key: 'field', label: 'Field', type: 'string', required: true },
            { key: 'operator', label: 'Operator', type: 'select', defaultValue: 'equals',
                options: [
                    { label: 'Equals', value: 'equals' },
                    { label: 'Contains', value: 'contains' },
                    { label: 'Starts With', value: 'starts_with' },
                    { label: 'Ends With', value: 'ends_with' },
                    { label: 'Not Empty', value: 'not_empty' },
                    { label: 'Empty', value: 'empty' },
                    { label: 'Greater Than', value: 'greater_than' },
                    { label: 'Less Than', value: 'less_than' },
                ] },
            { key: 'value', label: 'Value', type: 'string' },
        ],
        process: async (data, config) => {
            const result: any = await invoke('execute_processor_cmd', {
                request: { processor_type: 'rust-filter', data, config },
            });
            return result.success ? result.data : data;
        },
    },
};

export const sortPlugin: CrawlFlowPlugin = {
    id: 'rust-sort-ui',
    name: 'Sort (Rust)',
    version: '1.0.0',
    description: 'Sort data by a field (Rust backend).',
    author: 'CrawlFlow',
    capabilities: ['processor'],
    processor: {
        type: 'rust-sort',
        label: 'Sort',
        description: 'Sort rows by field',
        configFields: [
            { key: 'field', label: 'Field', type: 'string', required: true },
            { key: 'descending', label: 'Descending', type: 'boolean', defaultValue: false },
        ],
        process: async (data, config) => {
            const result: any = await invoke('execute_processor_cmd', {
                request: { processor_type: 'rust-sort', data, config },
            });
            return result.success ? result.data : data;
        },
    },
};

export const limitPlugin: CrawlFlowPlugin = {
    id: 'rust-limit-ui',
    name: 'Limit (Rust)',
    version: '1.0.0',
    description: 'Limit and offset data rows (Rust backend).',
    author: 'CrawlFlow',
    capabilities: ['processor'],
    processor: {
        type: 'rust-limit',
        label: 'Limit',
        description: 'Limit number of rows',
        configFields: [
            { key: 'count', label: 'Count', type: 'number', defaultValue: 100 },
            { key: 'offset', label: 'Offset', type: 'number', defaultValue: 0 },
        ],
        process: async (data, config) => {
            const result: any = await invoke('execute_processor_cmd', {
                request: { processor_type: 'rust-limit', data, config },
            });
            return result.success ? result.data : data;
        },
    },
};

export const builtinPlugins: CrawlFlowPlugin[] = [
    csvExportPlugin,
    jsonTransformPlugin,
    apiExportPlugin,
    rssDataSourcePlugin,
    dataAggregatorPlugin,
    htmlTableParserPlugin,
    deduplicatePlugin,
    filterPlugin,
    sortPlugin,
    limitPlugin,
];
