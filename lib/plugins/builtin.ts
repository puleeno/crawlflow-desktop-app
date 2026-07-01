import type { CrawlFlowPlugin } from '../../types';

// =====================================================
// CSV Export Plugin (hook capability)
// =====================================================
export const csvExportPlugin: CrawlFlowPlugin = {
    id: 'csv-export',
    name: 'CSV Export',
    version: '1.0.0',
    description: 'Export crawled data to CSV format with custom delimiter and header options.',
    author: 'CrawlFlow',
    capabilities: ['hook'],
    hooks: {
        customExport: async (ctx) => {
            const data = ctx.crawlData || [];
            const cfg = ctx.config;
            const delimiter = cfg.delimiter || ',';
            const includeHeader = cfg.includeHeader !== false;

            if (data.length === 0) {
                return { fileName: 'export.csv', mimeType: 'text/csv', content: '' };
            }

            const headers = Object.keys(data[0]);
            const rows: string[] = [];

            if (includeHeader) {
                rows.push(headers.map(h => `"${h}"`).join(delimiter));
            }
            for (const item of data) {
                const row = headers.map(h => `"${String(item[h] ?? '').replace(/"/g, '""')}"`);
                rows.push(row.join(delimiter));
            }

            const content = rows.join('\n');
            const fileName = `export_${Date.now()}.csv`;

            try {
                if (!!(window as any).__TAURI_INTERNALS__?.ipc) {
                    const { save } = await import('@tauri-apps/plugin-dialog');
                    const { writeTextFile } = await import('@tauri-apps/plugin-fs');
                    const path = await save({ defaultPath: fileName, filters: [{ name: 'CSV', extensions: ['csv'] }] });
                    if (path) await writeTextFile(path, content);
                } else {
                    downloadBlob(fileName, 'text/csv', content);
                }
            } catch {
                downloadBlob(fileName, 'text/csv', content);
            }
            return { fileName, mimeType: 'text/csv', content };
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
// JSON Transform Plugin (hook capability)
// =====================================================
export const jsonTransformPlugin: CrawlFlowPlugin = {
    id: 'json-transform',
    name: 'JSON Transform',
    version: '1.0.0',
    description: 'Transform crawled data using JavaScript expressions.',
    author: 'CrawlFlow',
    capabilities: ['hook'],
    hooks: {
        transformData: async (ctx) => {
            const data = ctx.crawlData || [];
            const rules = (ctx.config.rules as string) || '';
            if (!rules.trim()) return data;

            const parsed = rules.split('\n').filter(Boolean).map(line => {
                const eqIndex = line.indexOf('=');
                if (eqIndex === -1) return null;
                return { field: line.slice(0, eqIndex).trim(), expr: line.slice(eqIndex + 1).trim() };
            }).filter(Boolean) as { field: string; expr: string }[];

            return data.map((item: any) => {
                const r = { ...item };
                for (const { field, expr } of parsed) {
                    try { r[field] = new Function('data', `return (${expr});`)(item); } catch {}
                }
                return r;
            });
        },
    },
    configFields: [{
        key: 'rules', label: 'Transform Rules (field = expression)', type: 'textarea', required: true,
        placeholder: 'full_name = data.first_name + " " + data.last_name\nprice = parseFloat(data.price.replace("$",""))',
    }],
    defaultConfig: { rules: '' },
};

// =====================================================
// Send to API Plugin (hook capability)
// =====================================================
export const apiExportPlugin: CrawlFlowPlugin = {
    id: 'api-export',
    name: 'Send to API',
    version: '1.0.0',
    description: 'Send crawled data to a REST API endpoint in batches.',
    author: 'CrawlFlow',
    capabilities: ['hook'],
    hooks: {
        beforeSave: async (ctx) => {
            const data = ctx.crawlData || [];
            const endpoint = ctx.config.endpointUrl as string;
            if (!endpoint || data.length === 0) return data;

            const headers: Record<string, string> = { 'Content-Type': 'application/json' };
            if (ctx.config.authType === 'bearer') headers['Authorization'] = `Bearer ${ctx.config.bearerToken}`;
            if (ctx.config.authType === 'api-key') headers[ctx.config.apiKeyHeader as string || 'X-API-Key'] = ctx.config.apiKey as string;

            const batchSize = parseInt(ctx.config.batchSize as string) || 100;
            for (let i = 0; i < data.length; i += batchSize) {
                await fetch(endpoint, { method: 'POST', headers, body: JSON.stringify(data.slice(i, i + batchSize)) });
            }
            return data;
        },
    },
    configFields: [
        { key: 'endpointUrl', label: 'API Endpoint URL', type: 'string', required: true, placeholder: 'https://api.example.com/data' },
        { key: 'authType', label: 'Authentication', type: 'select', defaultValue: 'none',
            options: [{ label: 'None', value: 'none' }, { label: 'Bearer Token', value: 'bearer' }, { label: 'API Key', value: 'api-key' }] },
        { key: 'bearerToken', label: 'Bearer Token', type: 'string' },
        { key: 'apiKey', label: 'API Key', type: 'string' },
        { key: 'apiKeyHeader', label: 'API Key Header', type: 'string', defaultValue: 'X-API-Key' },
        { key: 'batchSize', label: 'Batch Size', type: 'number', defaultValue: 100 },
    ],
    defaultConfig: { endpointUrl: '', authType: 'none', batchSize: 100 },
};

// =====================================================
// RSS Feed Data Source Plugin (dataSource capability)
// =====================================================
export const rssDataSourcePlugin: CrawlFlowPlugin = {
    id: 'rss-feed-source',
    name: 'RSS Feed Source',
    version: '1.0.0',
    description: 'Fetch data from RSS/Atom feeds as a data source.',
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
            const url = config.feedUrl as string;
            if (!url) return [];

            const resp = await fetch(url);
            const text = await resp.text();
            const parser = new DOMParser();
            const xml = parser.parseFromString(text, 'text/xml');

            const items = xml.querySelectorAll('item, entry');
            const maxItems = parseInt(config.maxItems as string) || 50;
            const results: any[] = [];

            items.forEach((item, i) => {
                if (i >= maxItems) return;
                const getTag = (tag: string) => item.querySelector(tag)?.textContent || '';
                results.push({
                    title: getTag('title'),
                    link: getTag('link'),
                    description: getTag('description') || getTag('summary') || getTag('content\\:encoded'),
                    pubDate: getTag('pubDate') || getTag('published') || getTag('updated'),
                    author: getTag('author') || getTag('dc\\:creator'),
                    guid: getTag('guid') || getTag('id'),
                    categories: Array.from(item.querySelectorAll('category')).map(c => c.textContent).filter(Boolean),
                });
            });

            return results;
        },
    },
};

// =====================================================
// Data Aggregator Processor Plugin (processor capability)
// =====================================================
export const dataAggregatorPlugin: CrawlFlowPlugin = {
    id: 'data-aggregator',
    name: 'Data Aggregator',
    version: '1.0.0',
    description: 'Aggregate and summarize crawled data (count, sum, average, group by).',
    author: 'CrawlFlow',
    capabilities: ['processor'],
    processor: {
        type: 'data-aggregator',
        label: 'Data Aggregator',
        description: 'Aggregate data with group-by, count, sum, average operations',
        configFields: [
            { key: 'groupBy', label: 'Group By Field', type: 'string', placeholder: 'category' },
            { key: 'operation', label: 'Operation', type: 'select', defaultValue: 'count',
                options: [{ label: 'Count', value: 'count' }, { label: 'Sum', value: 'sum' }, { label: 'Average', value: 'avg' }] },
            { key: 'valueField', label: 'Value Field (for sum/avg)', type: 'string', placeholder: 'price' },
            { key: 'outputField', label: 'Output Field Name', type: 'string', defaultValue: 'result' },
        ],
        process: async (data, config) => {
            if (data.length === 0) return [];

            const groupBy = config.groupBy as string;
            const operation = config.operation as string || 'count';
            const valueField = config.valueField as string;
            const outputField = config.outputField as string || 'result';

            if (!groupBy) {
                const val = valueField ? data.reduce((s, d) => s + (parseFloat(d[valueField]) || 0), 0) : data.length;
                const count = operation === 'count' ? data.length
                    : operation === 'sum' ? val
                    : operation === 'avg' && valueField ? val / data.length : data.length;
                return [{ [outputField]: count, totalItems: data.length }];
            }

            const groups = new Map<string, any[]>();
            for (const item of data) {
                const key = String(item[groupBy] ?? 'unknown');
                if (!groups.has(key)) groups.set(key, []);
                groups.get(key)!.push(item);
            }

            return Array.from(groups.entries()).map(([key, items]) => {
                const vals = valueField ? items.map(d => parseFloat(d[valueField]) || 0) : [];
                const result = operation === 'count' ? items.length
                    : operation === 'sum' ? vals.reduce((a, b) => a + b, 0)
                    : operation === 'avg' ? vals.reduce((a, b) => a + b, 0) / (vals.length || 1)
                    : items.length;

                return { [groupBy]: key, [outputField]: result, totalItems: items.length };
            });
        },
    },
};

// =====================================================
// HTML Table Parser Plugin (parser capability)
// =====================================================
export const htmlTableParserPlugin: CrawlFlowPlugin = {
    id: 'html-table-parser',
    name: 'HTML Table Parser',
    version: '1.0.0',
    description: 'Parse HTML tables from web pages into structured data.',
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
            const parser = new DOMParser();
            const doc = parser.parseFromString(html, 'text/html');
            const tables = doc.querySelectorAll('table');
            const tableIndex = parseInt(config.tableIndex as string) || 0;

            if (tableIndex >= tables.length) return [];

            const table = tables[tableIndex];
            const rows = table.querySelectorAll('tr');
            const hasHeader = config.hasHeader !== false;
            const result: any[] = [];

            let headers: string[] = [];
            const rowStart = hasHeader ? 1 : 0;

            if (hasHeader && rows.length > 0) {
                headers = Array.from(rows[0].querySelectorAll('th, td')).map(c => c.textContent?.trim() || '');
            }

            for (let i = rowStart; i < rows.length; i++) {
                const cells = rows[i].querySelectorAll('td');
                const row: Record<string, string> = {};
                cells.forEach((cell, j) => {
                    const key = headers[j] || `col_${j}`;
                    row[key] = cell.textContent?.trim() || '';
                });
                if (Object.keys(row).length > 0) result.push(row);
            }

            return result;
        },
    },
};

// =====================================================
// Helper
// =====================================================
function downloadBlob(fileName: string, mimeType: string, content: string) {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = fileName;
    document.body.appendChild(a); a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
}

export const builtinPlugins: CrawlFlowPlugin[] = [
    csvExportPlugin,
    jsonTransformPlugin,
    apiExportPlugin,
    rssDataSourcePlugin,
    dataAggregatorPlugin,
    htmlTableParserPlugin,
];
