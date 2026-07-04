import { invoke } from '@tauri-apps/api/core';
import type { Preset } from '../types';

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

function getDefaultPresets(): Preset[] {
    return [
        {
            id: 'web-page-scraper',
            name: 'Web Page Scraper',
            description: 'Fetch a web page, extract content, and export to CSV.',
            icon: 'GlobeAltIcon',
            icon_color: '#22c55e',
            source: 'builtin',
            project_settings: {
                name: 'Web Scraper - {url}',
                description: 'Scrape web pages for structured data.',
                crawlDelay: 1000,
                userAgent: 'CrawlFlow/1.0',
                concurrency: 5,
            },
            nodes: [
                {
                    id: 'ds-1',
                    type: 'start',
                    label: 'From URL',
                    position: { x: 50, y: 50 },
                    data: { sourceType: 'url', sourceValue: '', urlSettings: { scope: 'current-url', excludeExtensions: ['pdf', 'jpg', 'png', 'zip', 'mp4', 'svg'], excludePatterns: [], whitelistPatterns: [], domainPolicy: 'all', domainWhitelist: [] } },
                },
                {
                    id: 'repo-1',
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
                    id: 'exp-1',
                    type: 'processor',
                    label: 'CSV Export',
                    position: { x: 50, y: 800 },
                    data: { processorType: 'generate-csv-file', processorConfig: { delimiter: ',', includeHeader: true } },
                },
            ],
            edges: [
                { id: 'e-ds-repo', source: 'ds-1', target: 'repo-1' },
                { id: 'e-repo-worker', source: 'repo-1', target: 'worker-1' },
                { id: 'e-ext-worker', source: 'ext-1', target: 'worker-1' },
                { id: 'e-worker-exp', source: 'worker-1', target: 'exp-1' },
            ],
        },
        {
            id: 'rss-monitor',
            name: 'RSS Feed Monitor',
            description: 'Monitor an RSS feed, filter items, and save to database.',
            icon: 'RssIcon',
            icon_color: '#f97316',
            source: 'builtin',
            project_settings: {
                name: 'RSS Monitor - {url}',
                description: 'Monitor RSS feeds for new content.',
                crawlDelay: 3600000,
                userAgent: 'CrawlFlow/1.0',
                concurrency: 2,
            },
            nodes: [
                {
                    id: 'ds-1',
                    type: 'start',
                    label: 'RSS Feed',
                    position: { x: 50, y: 50 },
                    data: { sourceType: 'url', sourceValue: '', pluginSourceType: 'py-rss', pluginConfig: {} },
                },
                {
                    id: 'repo-1',
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
                    label: 'Filter',
                    position: { x: 50, y: 800 },
                    data: { processorType: 'rust-filter', processorConfig: { field: 'title', operator: 'not_empty', value: '' } },
                },
                {
                    id: 'exp-1',
                    type: 'processor',
                    label: 'Save to DB',
                    position: { x: 400, y: 800 },
                    data: { processorType: 'save-to-database', processorConfig: { strategy: 'upsert' } },
                },
            ],
            edges: [
                { id: 'e-ds-repo', source: 'ds-1', target: 'repo-1' },
                { id: 'e-repo-worker', source: 'repo-1', target: 'worker-1' },
                { id: 'e-worker-proc', source: 'worker-1', target: 'proc-1' },
                { id: 'e-proc-exp', source: 'proc-1', target: 'exp-1' },
            ],
        },
        {
            id: 'web-page-to-excel',
            name: 'Web Page to Excel',
            description: 'Fetch a web page, extract content, and export to Excel (.xlsx).',
            icon: 'TableCellsIcon',
            icon_color: '#059669',
            source: 'builtin',
            project_settings: {
                name: 'Web to Excel - {url}',
                description: 'Scrape web pages to Excel spreadsheets.',
                crawlDelay: 1000,
                userAgent: 'CrawlFlow/1.0',
                concurrency: 5,
            },
            nodes: [
                {
                    id: 'ds-1',
                    type: 'start',
                    label: 'From URL',
                    position: { x: 50, y: 50 },
                    data: { sourceType: 'url', sourceValue: '', urlSettings: { scope: 'current-url', excludeExtensions: ['pdf', 'jpg', 'png', 'zip', 'mp4', 'svg'], excludePatterns: [], whitelistPatterns: [], domainPolicy: 'all', domainWhitelist: [] } },
                },
                {
                    id: 'repo-1',
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
                    id: 'exp-1',
                    type: 'processor',
                    label: 'Excel Export',
                    position: { x: 50, y: 800 },
                    data: { processorType: 'generate-excel-file', processorConfig: { sheetName: 'Sheet1', includeHeader: true } },
                },
            ],
            edges: [
                { id: 'e-ds-repo', source: 'ds-1', target: 'repo-1' },
                { id: 'e-repo-worker', source: 'repo-1', target: 'worker-1' },
                { id: 'e-ext-worker', source: 'ext-1', target: 'worker-1' },
                { id: 'e-worker-exp', source: 'worker-1', target: 'exp-1' },
            ],
        },
        {
            id: 'ecommerce-tracker',
            name: 'E-commerce Tracker',
            description: 'Track product prices and availability from e-commerce sites.',
            icon: 'ShoppingCartIcon',
            icon_color: '#6366f1',
            source: 'builtin',
            project_settings: {
                name: 'Price Tracker - {url}',
                description: 'Track e-commerce product prices.',
                crawlDelay: 86400000,
                userAgent: 'CrawlFlow/1.0',
                concurrency: 3,
            },
            nodes: [
                {
                    id: 'ds-1',
                    type: 'start',
                    label: 'Product URL',
                    position: { x: 50, y: 50 },
                    data: { sourceType: 'url', sourceValue: '', urlSettings: { scope: 'current-url', excludeExtensions: [], excludePatterns: [], whitelistPatterns: [], domainPolicy: 'all', domainWhitelist: [] } },
                },
                {
                    id: 'repo-1',
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
                    label: 'Extract Product',
                    position: { x: -40, y: 425 },
                    data: { presets: ['ecommerce-product'], customRules: [
                        { id: 'r1', name: 'Title', extractFrom: 'html-element', selector: 'h1', extract: 'text' },
                        { id: 'r2', name: 'Price', extractFrom: 'html-element', selector: '.price', extract: 'text' },
                        { id: 'r3', name: 'Availability', extractFrom: 'html-element', selector: '.stock', extract: 'text' },
                        { id: 'r4', name: 'Image', extractFrom: 'html-element', selector: '.product-image img', extract: 'attribute', attribute: 'src' },
                    ] },
                },
                {
                    id: 'proc-1',
                    type: 'processor',
                    label: 'Deduplicate',
                    position: { x: 50, y: 800 },
                    data: { processorType: 'rust-deduplicate', processorConfig: { field: 'Title' } },
                },
                {
                    id: 'exp-1',
                    type: 'processor',
                    label: 'CSV Export',
                    position: { x: 400, y: 800 },
                    data: { processorType: 'generate-csv-file', processorConfig: { delimiter: ',', includeHeader: true } },
                },
            ],
            edges: [
                { id: 'e-ds-repo', source: 'ds-1', target: 'repo-1' },
                { id: 'e-repo-worker', source: 'repo-1', target: 'worker-1' },
                { id: 'e-ext-worker', source: 'ext-1', target: 'worker-1' },
                { id: 'e-worker-proc', source: 'worker-1', target: 'proc-1' },
                { id: 'e-proc-exp', source: 'proc-1', target: 'exp-1' },
            ],
        },
    ];
}

export function getIconComponent(iconName: string): string {
    const icons: Record<string, string> = {
        GlobeAltIcon: '🌐',
        RssIcon: '📡',
        ShoppingCartIcon: '🛒',
    };
    return icons[iconName] || '📦';
}