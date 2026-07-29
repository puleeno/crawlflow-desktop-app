
// FIX: The content for this file was missing. These are the type definitions for the application.
import type { Node, NodeProps } from 'reactflow';
import type { ReactNode } from 'react';

export type DataSourceType = 'url' | 'api' | 'xml' | 'csv' | 'json' | 'mysql';
export type FileInputMethod = 'paste' | 'upload' | 'cloudUrl';

// ── Realtime service status & progress (mirrors src-tauri/src/services.rs) ──

export interface ServiceProgress {
    items_total: number;
    items_processed: number;
    items_success: number;
    items_failed: number;
    items_pending: number;
    progress_pct: number;
    phase: string;
    message: string;
    last_run_at: string;
}

export interface ServiceInfo {
    project_id: string;
    status: string;
    cycle_count: number;
    started_at: string;
    last_run_at: string;
    last_error: string | null;
    interval_seconds: number;
    progress: ServiceProgress;
    ws_port: number;
}

export interface MySQLConnection {
    host?: string;
    port?: string;
    user?: string;
    password?: string;
    database?: string;
}

export interface ProjectSettings {
    name: string;
    description: string;
    enabled: boolean;
    crawlDelay: number;
    userAgent: string;
    concurrency: number;
    executionMode: 'parallel' | 'queue';
    // Per-project export grouping (global export folder is configured in App Settings)
    groupExport?: boolean;
    groupFormat?: 'id' | 'name' | 'both';
    // Refresh strategy
    refreshStrategy?: 'refresh' | 'refresh_update' | 'update_only';
    updateMethod?: 'check_last_page' | 'check_first_page_until_duplicate';
    refreshInterval?: number; // seconds between cycles
}


// --- NEW TYPES FOR DATA SOURCE SETTINGS ---

export type CrawlScope = 'current-url' | 'entire-website';
export type DomainImportPolicy = 'all' | 'whitelist-only';

export interface HeaderPair {
    key: string;
    value: string;
}

export interface HttpClientConfig {
    clientType: 'reqwest' | 'chrome';
    userAgent?: string;
    timeoutSecs?: number;
    proxyUrl?: string;
    headers?: HeaderPair[];
    chromeArgs?: string[];
    waitForSelector?: string;
    headless?: boolean;
}

export interface URLSourceSettings {
    scope: CrawlScope;
    excludeExtensions: string[];
    excludePatterns: string[];
    whitelistPatterns: string[];
    domainPolicy: DomainImportPolicy;
    domainWhitelist: string[];
    httpClient?: HttpClientConfig;
}

export type APIAuthType = 'none' | 'api-key' | 'bearer' | 'basic';
export type APIKeyLocation = 'header' | 'query';

export interface APIKeyAuth {
    location: APIKeyLocation;
    keyName: string;
    keyValue: string;
}

export interface BearerTokenAuth {
    token: string;
}

export interface BasicAuth {
    username: string;
    password: string;
}

export type APIPaginationType = 'none' | 'page' | 'offset-limit' | 'next-url';

export interface PagePagination {
    paramName: string;
    startsAt: number;
}

export interface OffsetLimitPagination {
    offsetParam: string;
    limitParam: string;
    limitValue: number;
    startsAt: number;
}

export interface NextURLPagination {
    jsonPath: string; // Path to the next URL in the response
}

export interface APISourceSettings {
    authType: APIAuthType;
    authDetails: APIKeyAuth | BearerTokenAuth | BasicAuth | {};
    paginationType: APIPaginationType;
    paginationDetails: PagePagination | OffsetLimitPagination | NextURLPagination | {};
}

export interface XMLSourceSettings {
    scanUrls: boolean;
    domainPolicy: DomainImportPolicy;
    domainWhitelist: string[];
}

export type JSONDataHandling = 'raw' | 'scan-urls';
export type JSONURLSource = 'all-values' | 'specific-key';

export interface JSONSourceSettings {
    dataHandling: JSONDataHandling;
    urlSource?: JSONURLSource;
    urlKey?: string;
    domainPolicy?: DomainImportPolicy;
    domainWhitelist?: string[];
}

export interface StartNodeData {
    sourceType: DataSourceType;
    sourceValue: string | MySQLConnection;
    inputMethod?: FileInputMethod;
    fileName?: string;

    // New detailed settings
    urlSettings?: URLSourceSettings;
    apiSettings?: APISourceSettings;
    xmlSettings?: XMLSourceSettings;
    jsonSettings?: JSONSourceSettings;

    // Plugin data source
    pluginSourceType?: string;
    pluginConfig?: Record<string, any>;
}
// --- END OF NEW TYPES ---


export interface ClickNodeData {
    selector: string;
}

export type ExtractFrom = 'html-element' | 'json-ld' | 'html-comment';

export interface ExtractionRule {
    id: string;
    name: string;
    extractFrom: ExtractFrom;
    // For 'html-element'
    selector?: string;
    extract?: 'text' | 'attribute' | 'regex' | 'html';
    attribute?: string; // e.g., 'href', 'src', 'content'
    regexPattern?: string;
    regexGroup?: number;
    extractMultiple?: boolean;
    // For 'json-ld' and 'html-comment'
    jsonPath?: string;
}


export interface LoopNodeData {
    iteratorSelector: string;
}

export interface RepositoryNodeData {
    // This node serves as a structural element and may not need specific data.
}

/** Represents the Fetch/Get Data step – auto-created alongside the data source. */
export interface FetchDataNodeData {
    /** The origin data source type (url, api, csv, json, xml, mysql, plugin, etc.) */
    sourceType?: string;
    /** Human-readable label shown in the node */
    label?: string;
    /** URL patterns to filter which URLs to fetch (moved from preprocessor) */
    urlPatterns?: UrlPattern[];
    /** Client settings for fetching data */
    clientType?: string;
    clientTimeoutSecs?: number;
    clientHeadless?: boolean;
    waitForSelector?: string;
    waitTimeout?: number;
}

export interface UrlPattern {
    enabled: boolean;
    type: 'wildcard' | 'regex' | 'contains' | 'startswith' | 'endswith' | 'always';
    value: string;
}

export interface ExtractRule {
    type: string;
    value: string;
    attribute?: string | null;
}

export interface PreprocessorNodeData {
    inputType: 'html' | 'csv' | 'json' | 'xml' | 'text';
    itemSelector?: string;
    urlPatterns: UrlPattern[];
    extractRules: ExtractRule[];
    csvDelimiter?: string;
    csvHasHeader?: boolean;
    jsonItemPath?: string;
    pluginId?: string;
}

// FIX: Added missing ReceptionNodeData and ReceptionRule types to resolve import errors.
export interface ReceptionRule {
    id: string;
    // The exact properties are unknown as this feature seems incomplete.
    // Defining `id` is a safe assumption based on other rule types.
}

export interface ReceptionNodeData {
    rules: ReceptionRule[];
    logic: 'and' | 'or';
}

export type RuleCondition = 'exists' | 'not-exists' | 'contains' | 'not-contains' | 'matches-regex';

export type PresetType = 'woocommerce-product' | 'blog-post' | 'seo-metadata' | 'open-graph';

export interface HTMLDataExtractorNodeData {
    presets: PresetType[];
    customRules: ExtractionRule[];
    // Inspector related fields
    inspectorUrl?: string;
    inspectorHtmlContent?: string;
    inspectorLoading?: boolean;
    inspectorError?: string;
}

// --- NEW EXTRACTOR TYPES ---
export interface ColumnMapping {
    id: string;
    source: string; // Column Index for CSV, Column Name for MySQL
    fieldName: string;
}

export interface CSVExtractorNodeData {
    presets: string[];
    mappings: ColumnMapping[];
    hasHeader: boolean;
}

export interface PathMapping {
    id: string;
    path: string; // JSONPath or XPath
    fieldName: string;
}

export interface JSONExtractorNodeData {
    presets: string[];
    mappings: PathMapping[];
}

export interface XMLExtractorNodeData {
    presets: string[];
    mappings: PathMapping[];
}

export interface MySQLExtractorNodeData {
    presets: string[];
    mappings: ColumnMapping[];
}


// --- PROCESSOR SETTINGS ---
export interface SaveToDbSettings {
    connectionType: 'mysql' | 'postgresql';
    host?: string;
    port?: string;
    user?: string;
    password?: string;
    database?: string;
    tableName?: string;
    conflictStrategy: 'insert' | 'upsert' | 'skip';
    autoMapColumns: boolean;
    columnMapping: Record<string, string>; // key: extracted field, value: db column
}

export interface SendToApiSettings {
    endpointUrl: string;
    method: 'POST' | 'PUT' | 'PATCH';
    authType: 'none' | 'api-key' | 'bearer' | 'basic';
    authDetails: APIKeyAuth | BearerTokenAuth | BasicAuth | {};
    headers: { id: string; key: string; value: string }[];
    autoMapFields: boolean;
    fieldMapping: Record<string, string>; // key: extracted field, value: json key
}

export interface GenerateCsvSettings {
    fileName: string;
    delimiter: ',' | ';' | '\t';
    includeHeader: boolean;
    autoMapHeaders: boolean;
    columnMapping: Record<string, string>; // key: extracted field, value: csv header
}

export interface SendEmailSettings {
    recipients: string; // comma-separated
    subject: string;
    body: string;
    autoMapFields: boolean;
    fieldMapping: Record<string, string>; // key: extracted field, value: label in email
}

export interface GenerateExcelSettings {
    fileName: string;
    sheetName: string;
    includeHeader: boolean;
    autoMapHeaders: boolean;
    columnMapping: Record<string, string>; // key: extracted field, value: excel column
}

// Discriminated Union for ProcessorNodeData
export type ProcessorNodeData =
    | { processorType: 'save-to-database'; settings: SaveToDbSettings; }
    | { processorType: 'send-to-api'; settings: SendToApiSettings; }
    | { processorType: 'generate-csv-file'; settings: GenerateCsvSettings; }
    | { processorType: 'send-email-notification'; settings: SendEmailSettings; }
    | { processorType: 'generate-excel-file'; settings: GenerateExcelSettings; };


export type WorkerRuleType = 'url-format' | 'html-contains' | 'dom-value' | 'tag-attribute' | 'data-source-type';

export interface BaseWorkerRule {
    id: string;
    type: WorkerRuleType;
}

export interface URLFormatRule extends BaseWorkerRule {
    type: 'url-format';
    pattern: string;
}

export interface HTMLContainsRule extends BaseWorkerRule {
    type: 'html-contains';
    text: string;
}

export interface DOMValueRule extends BaseWorkerRule {
    type: 'dom-value';
    selector: string;
    condition: RuleCondition;
    value: string;
}

export interface TagAttributeRule extends BaseWorkerRule {
    type: 'tag-attribute';
    selector: string;
    attribute: string;
    condition: RuleCondition;
    value: string;
}

export interface DataSourceTypeRule extends BaseWorkerRule {
    type: 'data-source-type';
    sourceType: DataSourceType;
}

export type WorkerRule = URLFormatRule | HTMLContainsRule | DOMValueRule | TagAttributeRule | DataSourceTypeRule;

export interface WorkerNodeData {
    detectionRules: WorkerRule[];
    detectionLogic: 'and' | 'or';
    priority: number;
}

// FIX: Added CompletionNodeData interface to fix import error.
export interface CompletionNodeData {
    // This node marks the end of a flow, no specific data is needed.
}

// --- DIAGRAM SHAPE NODES ---
export type ShapeType = 'rectangle' | 'circle' | 'ellipse' | 'frame' | 'package';

export interface ShapeNodeData {
    shapeType: ShapeType;
    label: string;
    width: number;
    height: number;
    backgroundColor: string;
    borderColor: string;
    textColor: string;
}


// FIX: Added ReceptionNodeData to the NodeData union type.
// FIX: Added CompletionNodeData to the NodeData union type to resolve import error.
export type NodeData =
    | StartNodeData
    | ClickNodeData
    | LoopNodeData
    | RepositoryNodeData
    | FetchDataNodeData
    | PreprocessorNodeData
    | ReceptionNodeData
    | WorkerNodeData
    | HTMLDataExtractorNodeData
    | CSVExtractorNodeData
    | JSONExtractorNodeData
    | XMLExtractorNodeData
    | MySQLExtractorNodeData
    | ProcessorNodeData
    | CompletionNodeData
    | ShapeNodeData;


export type CustomNodeProps<T = NodeData> = NodeProps<T> & {
    title?: ReactNode;
};

export interface CustomNode extends Node<NodeData> {
    data: NodeData;
}

// --- PLUGIN SYSTEM ---

export type PluginCapability = 'hook' | 'dataSource' | 'processor' | 'parser';

export type PluginHook =
    | 'beforeCrawl'
    | 'afterExtract'
    | 'beforeSave'
    | 'afterSave'
    | 'transformData'
    | 'customExport';

export interface PluginConfigField {
    key: string;
    label: string;
    type: 'string' | 'number' | 'boolean' | 'select' | 'textarea';
    required?: boolean;
    defaultValue?: any;
    options?: { label: string; value: string }[];
    placeholder?: string;
}

export interface CrawlContext {
    sourceUrl?: string;
    sourceType: string;
    rawData?: any;
    config: Record<string, any>;
}

export interface PluginHookContext {
    crawlData?: any[];
    nodeId?: string;
    nodeType?: string;
    config: Record<string, any>;
}

export interface PluginExportResult {
    fileName: string;
    mimeType: string;
    content: string | Blob;
}

export interface DataSourceDefinition {
    type: string;
    label: string;
    description: string;
    icon?: string;
    configFields: PluginConfigField[];
    fetch: (config: Record<string, any>) => Promise<any[]>;
}

export interface ProcessorDefinition {
    type: string;
    label: string;
    description: string;
    icon?: string;
    configFields: PluginConfigField[];
    process: (data: any[], config: Record<string, any>) => Promise<any[]>;
}

export interface ParserDefinition {
    id: string;
    name: string;
    description: string;
    inputFormats: string[];
    configFields: PluginConfigField[];
    parse: (input: string | any, config: Record<string, any>) => Promise<any[]>;
    filter?: (data: any[], config: Record<string, any>) => Promise<any[]>;
}

export interface CrawlFlowPlugin {
    id: string;
    name: string;
    version: string;
    description: string;
    author?: string;
    icon?: string;
    capabilities: PluginCapability[];
    hooks?: Partial<Record<PluginHook, (ctx: PluginHookContext) => Promise<any>>>;
    dataSource?: DataSourceDefinition;
    processor?: ProcessorDefinition;
    parser?: ParserDefinition;
    configFields?: PluginConfigField[];
    defaultConfig?: Record<string, any>;
}

export interface PluginRecord {
    id: string;
    name: string;
    description: string;
    version: string;
    author?: string;
    type: string;
    config: string;
    enabled: number;
    installed_at: string;
}

export interface ProcessorPluginMapping {
    processorId: string;
    pluginId: string;
    hook: PluginHook;
    config: Record<string, any>;
    enabled: boolean;
}

export interface PresetNode {
    id: string;
    type: string;
    label: string;
    position: { x: number; y: number };
    data: Record<string, any>;
}

export interface PresetEdge {
    id: string;
    source: string;
    target: string;
    sourceHandle?: string;
    targetHandle?: string;
}

export interface RawItem {
    id: number;
    source_url: string;
    item_type: string;
    item_hash: string;
    raw_content: string | null;
    extracted_url: string | null;
    dup_count: number;
    priority: number;
    worker_id: string | null;
    matched: number;
    status: string;
    created_at: string;
    updated_at: string;
}

export interface ItemsSummary {
    total: number;
    pending: number;
    processing: number;
    done: number;
    error: number;
    ignored: number;
    crawled: number;
}

export interface Preset {
    id: string;
    name: string;
    description: string;
    icon: string;
    icon_color: string;
    source: 'builtin' | 'plugin';
    plugin_id?: string;
    project_settings: Partial<ProjectSettings>;
    nodes: PresetNode[];
    edges: PresetEdge[];
}
