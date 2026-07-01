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
                    position: { x: 50, y: 200 },
                    data: { sourceType: 'url', sourceValue: '', urlSettings: { scope: 'current-url', excludeExtensions: ['pdf', 'jpg', 'png', 'zip', 'mp4', 'svg'], excludePatterns: [], whitelistPatterns: [], domainPolicy: 'all', domainWhitelist: [] } },
                },
                {
                    id: 'ext-1',
                    type: 'htmlExtractor',
                    label: 'Extract Data',
                    position: { x: 350, y: 200 },
                    data: { presets: [], customRules: [] },
                },
                {
                    id: 'exp-1',
                    type: 'csvExport',
                    label: 'CSV Export',
                    position: { x: 650, y: 200 },
                    data: { presets: [], mappings: [], hasHeader: true },
                },
            ],
            edges: [
                { id: 'e-ds-ext', source: 'ds-1', target: 'ext-1', sourceHandle: 'data-out', targetHandle: 'data-in' },
                { id: 'e-ext-exp', source: 'ext-1', target: 'exp-1', sourceHandle: 'data-out', targetHandle: 'data-in' },
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
                    position: { x: 50, y: 200 },
                    data: { sourceType: 'url', sourceValue: '', pluginSourceType: 'py-rss', pluginConfig: {} },
                },
                {
                    id: 'proc-1',
                    type: 'processor',
                    label: 'Filter',
                    position: { x: 350, y: 200 },
                    data: { processorType: 'rust-filter', processorConfig: { field: 'title', operator: 'not_empty', value: '' } },
                },
                {
                    id: 'exp-1',
                    type: 'databaseExport',
                    label: 'Save to DB',
                    position: { x: 650, y: 200 },
                    data: { processorType: 'save-to-database', processorConfig: { strategy: 'upsert' } },
                },
            ],
            edges: [
                { id: 'e-ds-proc', source: 'ds-1', target: 'proc-1', sourceHandle: 'data-out', targetHandle: 'data-in' },
                { id: 'e-proc-exp', source: 'proc-1', target: 'exp-1', sourceHandle: 'data-out', targetHandle: 'data-in' },
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
                    position: { x: 50, y: 200 },
                    data: { sourceType: 'url', sourceValue: '', urlSettings: { scope: 'current-url', excludeExtensions: [], excludePatterns: [], whitelistPatterns: [], domainPolicy: 'all', domainWhitelist: [] } },
                },
                {
                    id: 'ext-1',
                    type: 'htmlExtractor',
                    label: 'Extract Product',
                    position: { x: 350, y: 150 },
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
                    position: { x: 350, y: 350 },
                    data: { processorType: 'rust-deduplicate', processorConfig: { field: 'Title' } },
                },
                {
                    id: 'exp-1',
                    type: 'csvExport',
                    label: 'CSV Export',
                    position: { x: 650, y: 200 },
                    data: { presets: [], mappings: [], hasHeader: true },
                },
            ],
            edges: [
                { id: 'e-ds-ext', source: 'ds-1', target: 'ext-1', sourceHandle: 'data-out', targetHandle: 'data-in' },
                { id: 'e-ext-proc', source: 'ext-1', target: 'proc-1', sourceHandle: 'data-out', targetHandle: 'data-in' },
                { id: 'e-proc-exp', source: 'proc-1', target: 'exp-1', sourceHandle: 'data-out', targetHandle: 'data-in' },
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