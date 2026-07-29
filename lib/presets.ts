import { invoke } from './platform';
import type { Preset } from '../types';

// This ID must match REPOSITORY_NODE_ID in App.tsx
const REPOSITORY_NODE_ID = 'repository-node';

let cachedPresets: Preset[] | null = null;

export async function listPresets(): Promise<Preset[]> {
    if (cachedPresets) return cachedPresets;

    try {
        const result: Preset[] = await invoke('list_presets_cmd');
        cachedPresets = result;
        return result;
    } catch (err) {
        console.warn('Failed to fetch presets from backend, using built-in defaults', err);
        return getDefaultPresets();
    }
}

/**
 * Built-in fallback presets used when the Tauri backend is unavailable (browser mode).
 *
 * FLOW RULES (enforced by App.tsx):
 *  1. start          → repository-node   (mandatory; repository is a singleton)
 *  2. repository-node → worker           (only valid target for repository)
 *  3. extractor      → worker            (extractors feed INTO worker, not the other way)
 *  4. worker         → processor         (one or more processor outputs)
 *  5. processor      → processor         (optional chaining)
 *  6. completion node is AUTO-MANAGED by the UI — do NOT include it in presets
 *
 * Node ID convention:
 *  - Repository node MUST use id = 'repository-node' to match REPOSITORY_NODE_ID constant.
 */
function getDefaultPresets(): Preset[] {
    return [
        // ── Preset 1: Web Page Scraper ─────────────────────────────────────────
        {
            id: 'web-page-scraper',
            name: 'Web Page Scraper',
            description: 'Fetch a web page, extract content with CSS selectors, and export to CSV.',
            icon: 'GlobeAltIcon',
            icon_color: '#22c55e',
            source: 'builtin',
            project_settings: {
                name: 'Web Scraper - {url}',
                description: 'Scrape web pages for structured data.',
                crawlDelay: 1000,
                userAgent: 'CrawlFlow/1.0',
                concurrency: 5,
                executionMode: 'queue',
            },
            nodes: [
                {
                    id: 'ds-1',
                    type: 'start',
                    label: 'From URL',
                    position: { x: 50, y: 50 },
                    data: {
                        sourceType: 'url',
                        sourceValue: '',
                        urlSettings: {
                            scope: 'current-url',
                            excludeExtensions: ['pdf', 'jpg', 'png', 'zip', 'mp4', 'svg'],
                            excludePatterns: [],
                            whitelistPatterns: [],
                            domainPolicy: 'all',
                            domainWhitelist: [],
                        },
                    },
                },
                {
                    // RULE: id must equal REPOSITORY_NODE_ID ('repository-node')
                    id: REPOSITORY_NODE_ID,
                    type: 'repository',
                    label: 'Raw Data Repository',
                    position: { x: 50, y: 300 },
                    data: {},
                },
                {
                    id: 'worker-1',
                    type: 'worker',
                    label: 'Data Router',
                    position: { x: 50, y: 550 },
                    data: {},
                },
                {
                    // RULE: extractor is placed above the worker and connects INTO it
                    id: 'ext-1',
                    type: 'html-data-extractor',
                    label: 'Extract Data',
                    position: { x: -40, y: 425 },
                    data: { presets: [], customRules: [] },
                },
                {
                    id: 'proc-1',
                    type: 'processor',
                    label: 'CSV Export',
                    position: { x: 50, y: 800 },
                    data: {
                        processorType: 'generate-csv-file',
                        processorConfig: { delimiter: ',', includeHeader: true },
                    },
                },
            ],
            edges: [
                { id: 'e-ds-repo',      source: 'ds-1',              target: REPOSITORY_NODE_ID },
                { id: 'e-repo-worker',  source: REPOSITORY_NODE_ID,  target: 'worker-1' },
                // RULE: extractor → worker (extractor feeds INTO worker)
                { id: 'e-ext-worker',   source: 'ext-1',             target: 'worker-1' },
                { id: 'e-worker-proc',  source: 'worker-1',          target: 'proc-1' },
            ],
        },

        // ── Preset 2: RSS Feed Monitor ─────────────────────────────────────────
        {
            id: 'rss-monitor',
            name: 'RSS Feed Monitor',
            description: 'Monitor an RSS feed, filter new items, and save to database.',
            icon: 'RssIcon',
            icon_color: '#f97316',
            source: 'builtin',
            project_settings: {
                name: 'RSS Monitor - {url}',
                description: 'Monitor RSS feeds for new content.',
                crawlDelay: 3600000,
                userAgent: 'CrawlFlow/1.0',
                concurrency: 2,
                executionMode: 'queue',
            },
            nodes: [
                {
                    id: 'ds-1',
                    type: 'start',
                    label: 'RSS Feed',
                    position: { x: 50, y: 50 },
                    data: {
                        sourceType: 'url',
                        sourceValue: '',
                        pluginSourceType: 'py-rss',
                        pluginConfig: {},
                    },
                },
                {
                    id: REPOSITORY_NODE_ID,
                    type: 'repository',
                    label: 'Raw Data Repository',
                    position: { x: 50, y: 300 },
                    data: {},
                },
                {
                    id: 'worker-1',
                    type: 'worker',
                    label: 'Data Router',
                    position: { x: 50, y: 550 },
                    data: {},
                },
                {
                    id: 'proc-1',
                    type: 'processor',
                    label: 'Filter (non-empty title)',
                    position: { x: 50, y: 800 },
                    data: {
                        processorType: 'rust-filter',
                        processorConfig: { field: 'title', operator: 'not_empty', value: '' },
                    },
                },
                {
                    id: 'proc-2',
                    type: 'processor',
                    label: 'Save to DB',
                    position: { x: 50, y: 1050 },
                    data: {
                        processorType: 'save-to-database',
                        processorConfig: { strategy: 'upsert' },
                    },
                },
            ],
            edges: [
                { id: 'e-ds-repo',      source: 'ds-1',             target: REPOSITORY_NODE_ID },
                { id: 'e-repo-worker',  source: REPOSITORY_NODE_ID, target: 'worker-1' },
                { id: 'e-worker-proc1', source: 'worker-1',         target: 'proc-1' },
                { id: 'e-proc1-proc2',  source: 'proc-1',           target: 'proc-2' },
            ],
        },

        // ── Preset 3: Web Page to Excel ────────────────────────────────────────
        {
            id: 'web-page-to-excel',
            name: 'Web Page to Excel',
            description: 'Fetch a web page, extract structured content, and export to Excel (.xlsx).',
            icon: 'TableCellsIcon',
            icon_color: '#059669',
            source: 'builtin',
            project_settings: {
                name: 'Web to Excel - {url}',
                description: 'Scrape web pages to Excel spreadsheets.',
                crawlDelay: 1000,
                userAgent: 'CrawlFlow/1.0',
                concurrency: 5,
                executionMode: 'queue',
            },
            nodes: [
                {
                    id: 'ds-1',
                    type: 'start',
                    label: 'From URL',
                    position: { x: 50, y: 50 },
                    data: {
                        sourceType: 'url',
                        sourceValue: '',
                        urlSettings: {
                            scope: 'current-url',
                            excludeExtensions: ['pdf', 'jpg', 'png', 'zip', 'mp4', 'svg'],
                            excludePatterns: [],
                            whitelistPatterns: [],
                            domainPolicy: 'all',
                            domainWhitelist: [],
                        },
                    },
                },
                {
                    id: REPOSITORY_NODE_ID,
                    type: 'repository',
                    label: 'Raw Data Repository',
                    position: { x: 50, y: 300 },
                    data: {},
                },
                {
                    id: 'worker-1',
                    type: 'worker',
                    label: 'Data Router',
                    position: { x: 50, y: 550 },
                    data: {},
                },
                {
                    id: 'ext-1',
                    type: 'html-data-extractor',
                    label: 'Extract Data',
                    position: { x: -40, y: 425 },
                    data: { presets: [], customRules: [] },
                },
                {
                    id: 'proc-1',
                    type: 'processor',
                    label: 'Excel Export',
                    position: { x: 50, y: 800 },
                    data: {
                        processorType: 'generate-excel-file',
                        processorConfig: { sheetName: 'Sheet1', includeHeader: true },
                    },
                },
            ],
            edges: [
                { id: 'e-ds-repo',      source: 'ds-1',             target: REPOSITORY_NODE_ID },
                { id: 'e-repo-worker',  source: REPOSITORY_NODE_ID, target: 'worker-1' },
                { id: 'e-ext-worker',   source: 'ext-1',            target: 'worker-1' },
                { id: 'e-worker-proc',  source: 'worker-1',         target: 'proc-1' },
            ],
        },

        // ── Preset 4: E-commerce Price Tracker ────────────────────────────────
        {
            id: 'ecommerce-tracker',
            name: 'E-commerce Tracker',
            description: 'Track product prices and availability from e-commerce product pages.',
            icon: 'ShoppingCartIcon',
            icon_color: '#6366f1',
            source: 'builtin',
            project_settings: {
                name: 'Price Tracker - {url}',
                description: 'Track e-commerce product prices.',
                crawlDelay: 86400000,
                userAgent: 'CrawlFlow/1.0',
                concurrency: 3,
                executionMode: 'queue',
            },
            nodes: [
                {
                    id: 'ds-1',
                    type: 'start',
                    label: 'Product URL',
                    position: { x: 50, y: 50 },
                    data: {
                        sourceType: 'url',
                        sourceValue: '',
                        urlSettings: {
                            scope: 'current-url',
                            excludeExtensions: [],
                            excludePatterns: [],
                            whitelistPatterns: [],
                            domainPolicy: 'all',
                            domainWhitelist: [],
                        },
                    },
                },
                {
                    id: REPOSITORY_NODE_ID,
                    type: 'repository',
                    label: 'Raw Data Repository',
                    position: { x: 50, y: 300 },
                    data: {},
                },
                {
                    id: 'worker-1',
                    type: 'worker',
                    label: 'Data Router',
                    position: { x: 50, y: 550 },
                    data: {},
                },
                {
                    id: 'ext-1',
                    type: 'html-data-extractor',
                    label: 'Extract Product Info',
                    position: { x: -40, y: 425 },
                    data: {
                        presets: ['ecommerce-product'],
                        customRules: [
                            { id: 'r1', name: 'Title',        extractFrom: 'html-element', selector: 'h1',                    extract: 'text' },
                            { id: 'r2', name: 'Price',        extractFrom: 'html-element', selector: '.price',                extract: 'text' },
                            { id: 'r3', name: 'Availability', extractFrom: 'html-element', selector: '.stock',                extract: 'text' },
                            { id: 'r4', name: 'Image',        extractFrom: 'html-element', selector: '.product-image img',   extract: 'attribute', attribute: 'src' },
                        ],
                    },
                },
                {
                    id: 'proc-1',
                    type: 'processor',
                    label: 'Deduplicate',
                    position: { x: 50, y: 800 },
                    data: {
                        processorType: 'rust-deduplicate',
                        processorConfig: { field: 'Title' },
                    },
                },
                {
                    id: 'proc-2',
                    type: 'processor',
                    label: 'CSV Export',
                    position: { x: 50, y: 1050 },
                    data: {
                        processorType: 'generate-csv-file',
                        processorConfig: { delimiter: ',', includeHeader: true },
                    },
                },
            ],
            edges: [
                { id: 'e-ds-repo',      source: 'ds-1',             target: REPOSITORY_NODE_ID },
                { id: 'e-repo-worker',  source: REPOSITORY_NODE_ID, target: 'worker-1' },
                { id: 'e-ext-worker',   source: 'ext-1',            target: 'worker-1' },
                { id: 'e-worker-proc1', source: 'worker-1',         target: 'proc-1' },
                { id: 'e-proc1-proc2',  source: 'proc-1',           target: 'proc-2' },
            ],
        },
    ];
}

export function getIconComponent(iconName: string): string {
    const icons: Record<string, string> = {
        GlobeAltIcon: '🌐',
        RssIcon: '📡',
        ShoppingCartIcon: '🛒',
        TableCellsIcon: '📊',
    };
    return icons[iconName] || '📦';
}