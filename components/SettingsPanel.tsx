
import React, { useState, useRef, ChangeEvent, useEffect, useMemo } from 'react';
import { Node, Edge } from 'reactflow';
import { invoke } from '../lib/platform';
import { XMarkIcon, Cog6ToothIcon, ArrowUpTrayIcon, ArrowDownTrayIcon, TrashIcon, ChevronDownIcon, ChevronUpIcon, CursorArrowRaysIcon, CloudIcon } from './icons';
import { NodeData, StartNodeData, ClickNodeData, ExtractionRule, FileInputMethod, MySQLConnection, ProjectSettings, LoopNodeData, WorkerNodeData, HTMLDataExtractorNodeData, ProcessorNodeData, PreprocessorNodeData, WorkerRule, WorkerRuleType, URLFormatRule, HTMLContainsRule, DOMValueRule, TagAttributeRule, ExtractFrom, URLSourceSettings, APISourceSettings, APIKeyAuth, BearerTokenAuth, BasicAuth, XMLSourceSettings, JSONSourceSettings, PagePagination, OffsetLimitPagination, NextURLPagination, RuleCondition, SaveToDbSettings, SendToApiSettings, GenerateCsvSettings, GenerateExcelSettings, SendEmailSettings, CSVExtractorNodeData, ColumnMapping, JSONExtractorNodeData, PathMapping, XMLExtractorNodeData, MySQLExtractorNodeData, ShapeNodeData, DataSourceTypeRule, HttpClientConfig, HeaderPair } from '../types';
import { PRESETS, PROCESSORS } from '../presets';
import { DynamicForm } from './settings/DynamicForm';
import type { SettingsSchema, FieldDef } from '../types/settings';
import ServiceControls from './ServiceControls';


interface SettingsPanelProps {
    node: Node | null;
    onUpdateNode: (nodeId: string, data: NodeData) => void;
    onDeleteNode: (nodeId: string) => void;
    onClose: () => void;
    projectSettings: ProjectSettings;
    onUpdateProjectSettings: (update: Partial<ProjectSettings>) => void;
    onExport: () => void;
    onSave: () => void;
    onImport: () => void;
    isOpen: boolean;
    // Inspector-related props
    onShowInspector: (htmlContent: string, baseUrl?: string) => void;
    onHideInspector: () => void;
    onStartPicking: (nodeId: string, ruleId: string) => void;
    onStopPicking: () => void;
    pickingRuleId: string | null;
    onInspectSelector: (selector: string | null) => void;
    highlightedSelector: string | null;
    nodes?: Node[];
    edges?: Edge[];
    projectId?: string | null;
    onOpenLogs?: () => void;
    isRunning?: boolean;
    serviceStatus?: string;
    serviceCycleCount?: number;
    serviceProgress?: any;
}

const commonInputClasses = "w-full p-2 bg-white text-gray-900 border border-slate-300 rounded-md shadow-sm placeholder:text-gray-400 focus:ring-2 focus:ring-blue-500 focus:border-blue-500 disabled:bg-slate-100 disabled:text-gray-500";
const commonSelectClasses = "w-full pl-3 pr-9 py-2 bg-white text-gray-900 border border-slate-300 rounded-md shadow-sm appearance-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 disabled:bg-slate-100 disabled:text-gray-500 cursor-pointer";
const smallSelectClasses = "w-full pl-2.5 pr-8 py-1.5 text-sm bg-white text-gray-900 border border-slate-300 rounded-md shadow-sm appearance-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 cursor-pointer disabled:bg-slate-100 disabled:text-gray-500";
const smallInputClasses = "w-full p-1.5 text-sm bg-white text-gray-900 border border-slate-300 rounded-md shadow-sm placeholder:text-gray-400 focus:ring-2 focus:ring-blue-500 focus:border-blue-500";
const commonLabelClasses = "block text-sm font-medium text-gray-700 mb-1";
const commonButtonClasses = "w-full p-2 rounded-md font-semibold text-white transition-colors";
const smallButtonClasses = "px-2.5 py-1.5 text-sm rounded-md font-semibold text-white transition-colors";

interface StyledSelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
    small?: boolean;
}

const StyledSelect: React.FC<StyledSelectProps> = ({ className, small, ...props }) => (
    <div className="relative w-full">
        <select
            {...props}
            className={`${small ? smallSelectClasses : commonSelectClasses} ${className ?? ''}`}
        />
        <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-gray-400">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" className="w-4 h-4">
                <path fillRule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clipRule="evenodd" />
            </svg>
        </div>
    </div>
);

const CollapsibleSection: React.FC<{ title: string; children: React.ReactNode; defaultOpen?: boolean }> = ({ title, children, defaultOpen = false }) => {
    const [isOpen, setIsOpen] = useState(defaultOpen);

    return (
        <div className="border-b border-slate-200">
            <button
                onClick={() => setIsOpen(!isOpen)}
                className="w-full flex justify-between items-center p-3 text-left font-semibold text-gray-800 hover:bg-slate-50"
            >
                <span>{title}</span>
                {isOpen ? <ChevronUpIcon /> : <ChevronDownIcon />}
            </button>
            {isOpen && <div className="p-4 bg-slate-50/70">{children}</div>}
        </div>
    );
};


const TagInput: React.FC<{ label: string; tags: string[]; onChange: (tags: string[]) => void; placeholder: string }> = ({ label, tags, onChange, placeholder }) => {
    const [inputValue, setInputValue] = useState('');

    const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
        if (e.key === 'Enter' || e.key === ',') {
            e.preventDefault();
            const newTag = inputValue.trim();
            if (newTag && !tags.includes(newTag)) {
                onChange([...tags, newTag]);
            }
            setInputValue('');
        }
    };

    const removeTag = (tagToRemove: string) => {
        onChange(tags.filter(tag => tag !== tagToRemove));
    };

    return (
        <div>
            <label className={commonLabelClasses}>{label}</label>
            <div className={`${commonInputClasses} flex flex-wrap items-center gap-2 h-auto`}>
                {tags.map((tag, index) => (
                    <span key={index} className="flex items-center gap-1 bg-blue-100 text-blue-800 text-xs font-semibold px-2 py-1 rounded-full">
                        {tag}
                        <button onClick={() => removeTag(tag)} className="text-blue-600 hover:text-blue-900">
                            <XMarkIcon />
                        </button>
                    </span>
                ))}
                <input
                    type="text"
                    value={inputValue}
                    onChange={(e) => setInputValue(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder={placeholder}
                    className="flex-grow bg-transparent outline-none border-none p-0 text-sm"
                />
            </div>
        </div>
    );
};


// --- Start Node Settings ---
const StartNodeSettings: React.FC<{ node: Node<StartNodeData>; onUpdate: (data: StartNodeData) => void }> = ({ node, onUpdate }) => {
    const { data } = node;

    const handleUpdate = <K extends keyof StartNodeData>(key: K, value: StartNodeData[K]) => {
        onUpdate({ ...data, [key]: value });
    };

    const handleNestedUpdate = (settingsKey: 'urlSettings' | 'apiSettings' | 'xmlSettings' | 'jsonSettings', update: any) => {
        onUpdate({
            ...data,
            [settingsKey]: {
                ...data[settingsKey],
                ...update,
            }
        });
    };

    const renderFileBasedInputs = () => {
        const inputMethod = data.inputMethod || 'paste';
        const fileInputRef = useRef<HTMLInputElement>(null);

        const handleFileChange = (e: ChangeEvent<HTMLInputElement>) => {
            const file = e.target.files?.[0];
            if (file) {
                const reader = new FileReader();
                reader.onload = (ev) => {
                    handleUpdate('sourceValue', ev.target?.result as string);
                    handleUpdate('fileName', file.name);
                };
                reader.readAsText(file);
            }
        };

        return (
            <div className="space-y-4">
                <div className="flex bg-slate-100 rounded-lg p-1">
                    {(['paste', 'upload', 'cloudUrl'] as FileInputMethod[]).map(method => (
                        <button
                            key={method}
                            onClick={() => handleUpdate('inputMethod', method)}
                            className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${inputMethod === method ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}
                        >
                            {method === 'paste' ? 'Paste' : method === 'upload' ? 'Upload' : 'Cloud URL'}
                        </button>
                    ))}
                </div>

                {inputMethod === 'paste' && (
                    <textarea
                        value={String(data.sourceValue)}
                        onChange={(e) => handleUpdate('sourceValue', e.target.value)}
                        placeholder={`Paste ${data.sourceType.toUpperCase()} content here`}
                        className={`${commonInputClasses} h-32 font-mono text-sm`}
                    />
                )}
                {inputMethod === 'upload' && (
                    <div>
                        <input type="file" ref={fileInputRef} onChange={handleFileChange} className="hidden" accept={`.${data.sourceType}`} />
                        <button onClick={() => fileInputRef.current?.click()} className={`${commonButtonClasses} bg-gray-600 hover:bg-gray-700 flex items-center justify-center gap-2`}>
                            <ArrowUpTrayIcon />
                            <span>{data.fileName || 'Choose a file'}</span>
                        </button>
                    </div>
                )}
                {inputMethod === 'cloudUrl' && (
                    <input
                        type="url"
                        value={String(data.sourceValue)}
                        onChange={(e) => handleUpdate('sourceValue', e.target.value)}
                        placeholder="https://example.com/data.json"
                        className={commonInputClasses}
                    />
                )}
            </div>
        );
    };

    const renderURLSettings = () => {
        const settings = data.urlSettings || {} as URLSourceSettings;
        return (
            <CollapsibleSection title="Crawl Settings" defaultOpen>
                <div className="space-y-4">
                    <div>
                        <label className={commonLabelClasses}>Crawl Scope</label>
                        <div className="flex bg-slate-100 rounded-lg p-1">
                            <button onClick={() => handleNestedUpdate('urlSettings', { scope: 'current-url' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.scope === 'current-url' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>Current URL Only</button>
                            <button onClick={() => handleNestedUpdate('urlSettings', { scope: 'entire-website' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.scope === 'entire-website' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>Entire Website</button>
                        </div>
                    </div>

                    <TagInput
                        label="Exclude Extensions"
                        tags={settings.excludeExtensions || []}
                        onChange={(tags) => handleNestedUpdate('urlSettings', { excludeExtensions: tags })}
                        placeholder="e.g., pdf, jpg, zip..."
                    />

                    <div>
                        <label className={commonLabelClasses}>Domain Import Policy</label>
                        <div className="flex bg-slate-100 rounded-lg p-1">
                            <button onClick={() => handleNestedUpdate('urlSettings', { domainPolicy: 'all' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.domainPolicy === 'all' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>All Domains</button>
                            <button onClick={() => handleNestedUpdate('urlSettings', { domainPolicy: 'whitelist-only' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.domainPolicy === 'whitelist-only' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>Whitelist Only</button>
                        </div>
                    </div>

                    {settings.domainPolicy === 'whitelist-only' && (
                        <TagInput
                            label="Domain Whitelist"
                            tags={settings.domainWhitelist || []}
                            onChange={(tags) => handleNestedUpdate('urlSettings', { domainWhitelist: tags })}
                            placeholder="e.g., example.com..."
                        />
                    )}
                </div>
            </CollapsibleSection>
        );
    };

    const renderHttpClientSettings = () => {
        const config = (data.urlSettings?.httpClient || { clientType: 'reqwest' }) as HttpClientConfig;

        const updateHttpClient = (update: Partial<HttpClientConfig>) => {
            const current = data.urlSettings || {} as URLSourceSettings;
            handleNestedUpdate('urlSettings', {
                httpClient: { ...config, ...update },
            });
        };

        const addHeader = () => {
            const headers = config.headers || [];
            updateHttpClient({ headers: [...headers, { key: '', value: '' }] });
        };

        const updateHeader = (index: number, field: 'key' | 'value', val: string) => {
            const headers = [...(config.headers || [])];
            headers[index] = { ...headers[index], [field]: val };
            updateHttpClient({ headers });
        };

        const removeHeader = (index: number) => {
            const headers = [...(config.headers || [])];
            headers.splice(index, 1);
            updateHttpClient({ headers });
        };

        return (
            <CollapsibleSection title="HTTP Client" defaultOpen={false}>
                <div className="space-y-4">
                    <div>
                        <label className={commonLabelClasses}>Client Type</label>
                        <StyledSelect value={config.clientType} onChange={e => updateHttpClient({ clientType: e.target.value as HttpClientConfig['clientType'] })}>
                            <option value="reqwest">Built-in (Reqwest)</option>
                            <option value="chrome">Headless Chrome</option>
                        </StyledSelect>
                    </div>
                    <div>
                        <label className={commonLabelClasses}>User-Agent</label>
                        <input type="text" value={config.userAgent ?? ''} onChange={e => updateHttpClient({ userAgent: e.target.value || undefined })} className={commonInputClasses} placeholder="CrawlFlow/1.0" />
                    </div>
                    <div>
                        <label className={commonLabelClasses}>Timeout (seconds)</label>
                        <input type="number" value={config.timeoutSecs ?? 30} onChange={e => updateHttpClient({ timeoutSecs: parseInt(e.target.value) || 30 })} className={commonInputClasses} min={1} max={300} />
                    </div>
                    <div>
                        <label className={commonLabelClasses}>Proxy URL</label>
                        <input type="text" value={config.proxyUrl ?? ''} onChange={e => updateHttpClient({ proxyUrl: e.target.value || undefined })} className={commonInputClasses} placeholder="e.g., http://127.0.0.1:8080" />
                    </div>
                    <div>
                        <div className="flex items-center justify-between mb-1">
                            <label className={commonLabelClasses}>Custom Headers</label>
                            <button type="button" onClick={addHeader} className="text-xs px-2 py-1 bg-blue-600 text-white rounded hover:bg-blue-700">+ Add</button>
                        </div>
                        <div className="space-y-2">
                            {(config.headers || []).map((h, i) => (
                                <div key={i} className="flex gap-2 items-start">
                                    <input type="text" value={h.key} onChange={e => updateHeader(i, 'key', e.target.value)} className={`${commonInputClasses} flex-1`} placeholder="Header" />
                                    <input type="text" value={h.value} onChange={e => updateHeader(i, 'value', e.target.value)} className={`${commonInputClasses} flex-1`} placeholder="Value" />
                                    <button type="button" onClick={() => removeHeader(i)} className="text-red-500 hover:text-red-700 text-lg leading-none mt-1">&times;</button>
                                </div>
                            ))}
                        </div>
                    </div>
                    {config.clientType === 'chrome' && (
                        <>
                            <div className="flex items-center gap-2">
                                <input
                                    type="checkbox"
                                    id="chromeHeadless"
                                    checked={config.headless !== false}
                                    onChange={e => updateHttpClient({ headless: e.target.checked })}
                                    className="h-4 w-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                                />
                                <label htmlFor="chromeHeadless" className="text-sm text-gray-700">Headless Mode</label>
                            </div>
                            <div>
                                <label className={commonLabelClasses}>Wait for Selector</label>
                                <input type="text" value={config.waitForSelector ?? ''} onChange={e => updateHttpClient({ waitForSelector: e.target.value || undefined })} className={commonInputClasses} placeholder="e.g., .content-loaded" />
                            </div>
                            <TagInput
                                label="Chrome Arguments"
                                tags={config.chromeArgs || []}
                                onChange={(tags) => updateHttpClient({ chromeArgs: tags })}
                                placeholder="e.g., --disable-web-security"
                            />
                        </>
                    )}
                </div>
            </CollapsibleSection>
        );
    };

    const renderAPISettings = () => {
        const settings = data.apiSettings || {} as APISourceSettings;
        const authDetails = settings.authDetails || {};

        return (
            <>
                <CollapsibleSection title="Authentication" defaultOpen>
                    <div className="space-y-3">
                        <label className={commonLabelClasses}>Auth Type</label>
                        <StyledSelect value={settings.authType} onChange={(e) => handleNestedUpdate('apiSettings', { authType: e.target.value as any, authDetails: {} })}>
                            <option value="none">None</option>
                            <option value="api-key">API Key</option>
                            <option value="bearer">Bearer Token</option>
                            <option value="basic">Basic Auth</option>
                        </StyledSelect>
                        {settings.authType === 'api-key' && (
                            <div className="p-3 bg-slate-100 rounded-md space-y-3">
                                <label className={commonLabelClasses}>Location</label>
                                <StyledSelect value={(authDetails as APIKeyAuth).location} onChange={(e) => handleNestedUpdate('apiSettings', { authDetails: { ...(authDetails as APIKeyAuth), location: e.target.value as any } })}>
                                    <option value="header">Header</option>
                                    <option value="query">Query Parameter</option>
                                </StyledSelect>
                                <label className={commonLabelClasses}>Key Name</label>
                                <input type="text" value={(authDetails as APIKeyAuth).keyName || ''} onChange={e => handleNestedUpdate('apiSettings', { authDetails: { ...(authDetails as APIKeyAuth), keyName: e.target.value } })} placeholder="X-API-KEY" className={commonInputClasses} />
                                <label className={commonLabelClasses}>Key Value</label>
                                <input type="password" value={(authDetails as APIKeyAuth).keyValue || ''} onChange={e => handleNestedUpdate('apiSettings', { authDetails: { ...(authDetails as APIKeyAuth), keyValue: e.target.value } })} placeholder="your-api-key" className={commonInputClasses} />
                            </div>
                        )}
                        {settings.authType === 'bearer' && (
                            <div className="p-3 bg-slate-100 rounded-md space-y-3">
                                <label className={commonLabelClasses}>Bearer Token</label>
                                <input type="password" value={(authDetails as BearerTokenAuth).token || ''} onChange={e => handleNestedUpdate('apiSettings', { authDetails: { ...(authDetails as BearerTokenAuth), token: e.target.value } })} placeholder="your-bearer-token" className={commonInputClasses} />
                            </div>
                        )}
                        {settings.authType === 'basic' && (
                            <div className="p-3 bg-slate-100 rounded-md space-y-3">
                                <label className={commonLabelClasses}>Username</label>
                                <input type="text" value={(authDetails as BasicAuth).username || ''} onChange={e => handleNestedUpdate('apiSettings', { authDetails: { ...(authDetails as BasicAuth), username: e.target.value } })} placeholder="username" className={commonInputClasses} />
                                <label className={commonLabelClasses}>Password</label>
                                <input type="password" value={(authDetails as BasicAuth).password || ''} onChange={e => handleNestedUpdate('apiSettings', { authDetails: { ...(authDetails as BasicAuth), password: e.target.value } })} placeholder="password" className={commonInputClasses} />
                            </div>
                        )}
                    </div>
                </CollapsibleSection>
                <CollapsibleSection title="Pagination">
                    <div className="space-y-3">
                        <label className={commonLabelClasses}>Pagination Type</label>
                        <StyledSelect value={settings.paginationType} onChange={(e) => handleNestedUpdate('apiSettings', { paginationType: e.target.value as any, paginationDetails: {} })}>
                            <option value="none">None</option>
                            <option value="page">Page Number</option>
                            <option value="offset-limit">Offset/Limit</option>
                            <option value="next-url">Next URL Path</option>
                        </StyledSelect>
                        {settings.paginationType === 'page' && (
                            <div className="p-3 bg-slate-100 rounded-md space-y-3">
                                <p className="text-xs text-gray-600">Use {'`{{page}}`'} tag in the API URL.</p>
                                <label className={commonLabelClasses}>Parameter Name</label>
                                <input type="text" value={(settings.paginationDetails as PagePagination).paramName || 'page'} onChange={e => handleNestedUpdate('apiSettings', { paginationDetails: { ...(settings.paginationDetails as PagePagination), paramName: e.target.value } })} className={commonInputClasses} />
                                <label className={commonLabelClasses}>Starts At</label>
                                <input type="number" value={(settings.paginationDetails as PagePagination).startsAt || 1} onChange={e => handleNestedUpdate('apiSettings', { paginationDetails: { ...(settings.paginationDetails as PagePagination), startsAt: parseInt(e.target.value) } })} className={commonInputClasses} />
                            </div>
                        )}
                        {settings.paginationType === 'offset-limit' && (
                            <div className="p-3 bg-slate-100 rounded-md space-y-3">
                                <p className="text-xs text-gray-600">Use {'`{{offset}}`'} and {'`{{limit}}`'} tags in the API URL.</p>
                                <label className={commonLabelClasses}>Offset Param Name</label>
                                <input type="text" value={(settings.paginationDetails as OffsetLimitPagination).offsetParam || 'offset'} onChange={e => handleNestedUpdate('apiSettings', { paginationDetails: { ...(settings.paginationDetails as OffsetLimitPagination), offsetParam: e.target.value } })} className={commonInputClasses} />
                                <label className={commonLabelClasses}>Limit Param Name</label>
                                <input type="text" value={(settings.paginationDetails as OffsetLimitPagination).limitParam || 'limit'} onChange={e => handleNestedUpdate('apiSettings', { paginationDetails: { ...(settings.paginationDetails as OffsetLimitPagination), limitParam: e.target.value } })} className={commonInputClasses} />
                                <label className={commonLabelClasses}>Limit Value</label>
                                <input type="number" value={(settings.paginationDetails as OffsetLimitPagination).limitValue || 100} onChange={e => handleNestedUpdate('apiSettings', { paginationDetails: { ...(settings.paginationDetails as OffsetLimitPagination), limitValue: parseInt(e.target.value) } })} className={commonInputClasses} />
                                <label className={commonLabelClasses}>Starts At</label>
                                <input type="number" value={(settings.paginationDetails as OffsetLimitPagination).startsAt || 0} onChange={e => handleNestedUpdate('apiSettings', { paginationDetails: { ...(settings.paginationDetails as OffsetLimitPagination), startsAt: parseInt(e.target.value) } })} className={commonInputClasses} />
                            </div>
                        )}
                        {settings.paginationType === 'next-url' && (
                            <div className="p-3 bg-slate-100 rounded-md space-y-3">
                                <label className={commonLabelClasses}>JSON Path to Next URL</label>
                                <input type="text" value={(settings.paginationDetails as NextURLPagination).jsonPath || ''} onChange={e => handleNestedUpdate('apiSettings', { paginationDetails: { ...(settings.paginationDetails as NextURLPagination), jsonPath: e.target.value } })} placeholder="e.g., meta.pagination.next_url" className={commonInputClasses} />
                            </div>
                        )}
                    </div>
                </CollapsibleSection>
            </>
        );
    };

    const renderXMLSettings = () => {
        const settings = data.xmlSettings || {} as XMLSourceSettings;
        return (
            <CollapsibleSection title="XML Settings" defaultOpen>
                <div className="space-y-4">
                    <div className="flex items-center gap-2">
                        <input
                            type="checkbox"
                            id="scanUrls"
                            checked={settings.scanUrls}
                            onChange={(e) => handleNestedUpdate('xmlSettings', { scanUrls: e.target.checked })}
                            className="h-4 w-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                        />
                        <label htmlFor="scanUrls" className="text-sm text-gray-700">Scan for URLs from sitemap/feed</label>
                    </div>

                    <div>
                        <label className={commonLabelClasses}>Domain Import Policy</label>
                        <div className="flex bg-slate-100 rounded-lg p-1">
                            <button onClick={() => handleNestedUpdate('xmlSettings', { domainPolicy: 'all' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.domainPolicy === 'all' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>All Domains</button>
                            <button onClick={() => handleNestedUpdate('xmlSettings', { domainPolicy: 'whitelist-only' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.domainPolicy === 'whitelist-only' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>Whitelist Only</button>
                        </div>
                    </div>

                    {settings.domainPolicy === 'whitelist-only' && (
                        <TagInput
                            label="Domain Whitelist"
                            tags={settings.domainWhitelist || []}
                            onChange={(tags) => handleNestedUpdate('xmlSettings', { domainWhitelist: tags })}
                            placeholder="e.g., example.com..."
                        />
                    )}
                </div>
            </CollapsibleSection>
        )
    };

    const renderJSONSettings = () => {
        const settings = data.jsonSettings || {} as JSONSourceSettings;
        return (
            <CollapsibleSection title="JSON Settings" defaultOpen>
                <div className="space-y-4">
                    <div>
                        <label className={commonLabelClasses}>Data Handling</label>
                        <div className="flex bg-slate-100 rounded-lg p-1">
                            <button onClick={() => handleNestedUpdate('jsonSettings', { dataHandling: 'raw' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.dataHandling === 'raw' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>Treat as Raw Data</button>
                            <button onClick={() => handleNestedUpdate('jsonSettings', { dataHandling: 'scan-urls' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.dataHandling === 'scan-urls' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>Scan for URLs</button>
                        </div>
                    </div>
                    {settings.dataHandling === 'scan-urls' && (
                        <div className="p-3 bg-slate-100 rounded-md space-y-4">
                            <div>
                                <label className={commonLabelClasses}>URL Source</label>
                                <div className="flex bg-slate-100 rounded-lg p-1">
                                    <button onClick={() => handleNestedUpdate('jsonSettings', { urlSource: 'all-values' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.urlSource === 'all-values' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>Scan All Values</button>
                                    <button onClick={() => handleNestedUpdate('jsonSettings', { urlSource: 'specific-key' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.urlSource === 'specific-key' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>From Specific Key</button>
                                </div>
                            </div>
                            {settings.urlSource === 'specific-key' && (
                                <div>
                                    <label htmlFor="urlKey" className={commonLabelClasses}>URL Key Name</label>
                                    <input id="urlKey" type="text" value={settings.urlKey || ''} onChange={e => handleNestedUpdate('jsonSettings', { urlKey: e.target.value })} placeholder="e.g., productUrl" className={commonInputClasses} />
                                </div>
                            )}

                            <div>
                                <label className={commonLabelClasses}>Domain Import Policy</label>
                                <div className="flex bg-slate-100 rounded-lg p-1">
                                    <button onClick={() => handleNestedUpdate('jsonSettings', { domainPolicy: 'all' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.domainPolicy === 'all' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>All Domains</button>
                                    <button onClick={() => handleNestedUpdate('jsonSettings', { domainPolicy: 'whitelist-only' })} className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${settings.domainPolicy === 'whitelist-only' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}>Whitelist Only</button>
                                </div>
                            </div>

                            {settings.domainPolicy === 'whitelist-only' && (
                                <TagInput
                                    label="Domain Whitelist"
                                    tags={settings.domainWhitelist || []}
                                    onChange={(tags) => handleNestedUpdate('jsonSettings', { domainWhitelist: tags })}
                                    placeholder="e.g., example.com..."
                                />
                            )}
                        </div>
                    )}
                </div>
            </CollapsibleSection>
        )
    };


    const isFileBased = ['xml', 'csv', 'json'].includes(data.sourceType);

    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">Start Node Settings</h3>
            {isFileBased ? (
                renderFileBasedInputs()
            ) : data.sourceType === 'mysql' ? (
                <div className="space-y-2">
                    {(Object.keys(data.sourceValue) as Array<keyof MySQLConnection>).map(key => (
                        <div key={key}>
                            <label htmlFor={key} className={commonLabelClasses}>{key.charAt(0).toUpperCase() + key.slice(1)}</label>
                            <input
                                id={key}
                                type={key === 'password' ? 'password' : 'text'}
                                value={(data.sourceValue as MySQLConnection)[key] || ''}
                                onChange={e => handleUpdate('sourceValue', { ...(data.sourceValue as MySQLConnection), [key]: e.target.value })}
                                className={commonInputClasses}
                            />
                        </div>
                    ))}
                </div>
            ) : (
                <div>
                    <label htmlFor="sourceValue" className={commonLabelClasses}>Start URL / API Endpoint</label>
                    <input
                        id="sourceValue"
                        type="text"
                        value={String(data.sourceValue)}
                        onChange={e => handleUpdate('sourceValue', e.target.value)}
                        className={commonInputClasses}
                    />
                </div>
            )}

            {data.sourceType === 'url' && renderURLSettings()}
            {data.sourceType === 'url' && renderHttpClientSettings()}
            {data.sourceType === 'api' && renderAPISettings()}
            {data.sourceType === 'xml' && renderXMLSettings()}
            {data.sourceType === 'json' && renderJSONSettings()}
        </div>
    );
};

// --- Other Node Settings ---
const ClickNodeSettings: React.FC<{ node: Node<ClickNodeData>; onUpdate: (data: ClickNodeData) => void }> = ({ node, onUpdate }) => {
    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">Click Node Settings</h3>
            <div>
                <label htmlFor="selector" className={commonLabelClasses}>CSS Selector</label>
                <input
                    id="selector"
                    type="text"
                    value={node.data.selector}
                    onChange={e => onUpdate({ ...node.data, selector: e.target.value })}
                    className={commonInputClasses}
                    placeholder="e.g., a.next-page, button#load-more"
                />
            </div>
        </div>
    );
};

const LoopNodeSettings: React.FC<{ node: Node<LoopNodeData>; onUpdate: (data: LoopNodeData) => void }> = ({ node, onUpdate }) => {
    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">Loop Node Settings</h3>
            <div>
                <label htmlFor="iteratorSelector" className={commonLabelClasses}>Iterator CSS Selector</label>
                <input
                    id="iteratorSelector"
                    type="text"
                    value={node.data.iteratorSelector}
                    onChange={e => onUpdate({ ...node.data, iteratorSelector: e.target.value })}
                    className={commonInputClasses}
                    placeholder="e.g., .product-list .item"
                />
                <p className="text-xs text-gray-500 mt-1">This selector defines the elements to loop over. Other nodes can be dragged inside this node on the canvas.</p>
            </div>
        </div>
    );
};

const HTMLDataExtractorSettings: React.FC<{
    node: Node<HTMLDataExtractorNodeData>;
    onUpdate: (data: HTMLDataExtractorNodeData) => void;
    props: Omit<SettingsPanelProps, 'onAddNode' | 'onAddShapeNode'>;
}> = ({ node, onUpdate, props }) => {
    const { data } = node;
    const [inspectorInputMethod, setInspectorInputMethod] = useState<'url' | 'paste'>('url');
    const [pastedHtml, setPastedHtml] = useState(data.inspectorHtmlContent || '');

    const handleRuleChange = (ruleId: string, field: keyof ExtractionRule, value: any) => {
        const newRules = data.customRules.map(r => r.id === ruleId ? { ...r, [field]: value } : r);
        onUpdate({ ...data, customRules: newRules });
    };

    const addRule = () => {
        const newRule: ExtractionRule = { id: `${Date.now()}`, name: `field_${data.customRules.length + 1}`, extractFrom: 'html-element', selector: '', extract: 'text' };
        onUpdate({ ...data, customRules: [...data.customRules, newRule] });
    };

    const removeRule = (ruleId: string) => {
        onUpdate({ ...data, customRules: data.customRules.filter(r => r.id !== ruleId) });
    };

    const togglePreset = (presetKey: string) => {
        const currentPresets = data.presets || [];
        const preset = PRESETS[presetKey]?.html;
        if (!preset) return;

        const isCurrentlySelected = currentPresets.includes(presetKey as any);

        let newRules = [...(data.customRules || [])];

        if (isCurrentlySelected) {
            // Deselecting: remove this preset's rules
            const presetRuleIds = new Set(preset.rules.map(r => r.id));
            newRules = newRules.filter(r => !presetRuleIds.has(r.id));
            const newPresets = currentPresets.filter(p => p !== (presetKey as any));
            onUpdate({ ...data, presets: newPresets, customRules: newRules });
        } else {
            // Selecting: add this preset's rules (if not already present by ID)
            const existingRuleIds = new Set(newRules.map(r => r.id));
            const rulesToAdd = preset.rules.filter(r => !existingRuleIds.has(r.id));
            newRules = [...newRules, ...rulesToAdd];
            const newPresets = [...currentPresets, presetKey as any];
            onUpdate({ ...data, presets: newPresets, customRules: newRules });
        }
    };

    // Memoized set of preset rule IDs for efficient lookup during render
    const presetRuleIds = useMemo(() => {
        const ids = new Set<string>();
        (data.presets || []).forEach(presetKey => {
            PRESETS[presetKey]?.html?.rules.forEach(rule => {
                ids.add(rule.id);
            });
        });
        return ids;
    }, [data.presets]);

    const handleFetchHtml = async () => {
        if (!data.inspectorUrl) {
            onUpdate({ ...data, inspectorError: 'Please enter a URL to inspect.' });
            return;
        }
        onUpdate({ ...data, inspectorLoading: true, inspectorError: undefined, inspectorHtmlContent: undefined });
        try {
            const result: any = await invoke('fetch_url_cmd', {
                request: {
                    url: data.inspectorUrl,
                    method: 'GET',
                },
            });
            const htmlContent = result.html;
            if (!htmlContent) {
                throw new Error(result.error || 'No HTML content returned');
            }
            onUpdate({ ...data, inspectorLoading: false, inspectorHtmlContent: htmlContent });
            props.onShowInspector(htmlContent, data.inspectorUrl);
        } catch (error: any) {
            console.error("Failed to fetch HTML:", error);
            const errorMessage = typeof error === 'string'
                ? error
                : 'Failed to fetch HTML. The URL may be invalid, the site may be down, or it might be blocking requests. Please try the "Paste HTML" option instead.';
            onUpdate({ ...data, inspectorLoading: false, inspectorError: errorMessage });
        }
    };

    const handleLoadPastedHtml = () => {
        if (!pastedHtml) {
            onUpdate({ ...data, inspectorError: 'Please paste HTML content to load.' });
            return;
        }
        onUpdate({ ...data, inspectorLoading: false, inspectorHtmlContent: pastedHtml, inspectorError: undefined });
        props.onShowInspector(pastedHtml);
    };

    useEffect(() => {
        // If panel is closed while inspector is open, hide it
        return () => {
            props.onHideInspector();
        }
    }, []);

    const availablePresets = useMemo(() => {
        return Object.entries(PRESETS).filter(([_, preset]) => !!preset.html);
    }, []);

    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">HTML Data Extractor Settings</h3>

            <CollapsibleSection title="Inspector Tool" defaultOpen>
                <div className="space-y-2">
                    <p className="text-xs text-gray-600 mb-2">
                        Load HTML to visually select elements. Fetching from a URL may fail due to browser security (CORS). If that happens, use the Paste HTML option.
                    </p>
                    <div className="flex bg-slate-100 rounded-lg p-1">
                        <button
                            onClick={() => setInspectorInputMethod('url')}
                            className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${inspectorInputMethod === 'url' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}
                        >
                            Fetch from URL
                        </button>
                        <button
                            onClick={() => setInspectorInputMethod('paste')}
                            className={`flex-1 p-2 text-sm font-semibold rounded-md transition-colors ${inspectorInputMethod === 'paste' ? 'bg-white text-blue-600 shadow-sm' : 'text-slate-600 hover:bg-white/60'}`}
                        >
                            Paste HTML
                        </button>
                    </div>

                    {inspectorInputMethod === 'url' ? (
                        <div className="flex gap-2 pt-2">
                            <input
                                type="url"
                                placeholder="https://example.com/product/123"
                                value={data.inspectorUrl || ''}
                                onChange={(e) => onUpdate({ ...data, inspectorUrl: e.target.value })}
                                className={commonInputClasses}
                            />
                            <button onClick={handleFetchHtml} disabled={data.inspectorLoading} className={`${smallButtonClasses} bg-blue-600 hover:bg-blue-700 disabled:bg-blue-300`}>
                                {data.inspectorLoading ? 'Loading...' : 'Fetch'}
                            </button>
                        </div>
                    ) : (
                        <div className="pt-2">
                            <textarea
                                placeholder="Paste the full HTML source code here"
                                value={pastedHtml}
                                onChange={(e) => setPastedHtml(e.target.value)}
                                className={`${commonInputClasses} h-32 font-mono text-sm`}
                            />
                            <button onClick={handleLoadPastedHtml} className={`${commonButtonClasses} bg-blue-600 hover:bg-blue-700 mt-2`}>
                                Load HTML
                            </button>
                        </div>
                    )}
                    {data.inspectorError && <p className="text-sm text-red-600 mt-2">{data.inspectorError}</p>}
                </div>
            </CollapsibleSection>

            <CollapsibleSection title="Extraction Presets">
                <div className="grid grid-cols-2 gap-2">
                    {availablePresets.map(([key, preset]) => (
                        <div key={key} className="flex items-center gap-2">
                            <input
                                type="checkbox"
                                id={`preset-${key}`}
                                checked={(data.presets || []).includes(key as any)}
                                onChange={() => togglePreset(key)}
                                className="h-4 w-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                            />
                            <label htmlFor={`preset-${key}`} className="text-sm text-gray-700">{preset.name}</label>
                        </div>
                    ))}
                </div>
            </CollapsibleSection>

            <CollapsibleSection title="Custom Extraction Rules" defaultOpen>
                <div className="space-y-3">
                    {data.customRules.map((rule) => {
                        const isPresetRule = presetRuleIds.has(rule.id);
                        return (
                            <div key={rule.id} className={`p-3 border rounded-lg space-y-2 relative ${isPresetRule ? 'bg-blue-50 border-blue-200' : 'bg-slate-50 border-slate-200'}`}>
                                {isPresetRule ? (
                                    <span className="absolute top-2 right-2 text-xs font-semibold text-blue-700 bg-blue-200 px-2 py-0.5 rounded-full">Preset</span>
                                ) : (
                                    <button onClick={() => removeRule(rule.id)} className="absolute top-2 right-2 text-gray-400 hover:text-red-500"><TrashIcon /></button>
                                )}
                                <div className="grid grid-cols-2 gap-2">
                                    <input type="text" placeholder="Field Name" value={rule.name} onChange={e => handleRuleChange(rule.id, 'name', e.target.value)} className={`${smallInputClasses} disabled:bg-slate-200 disabled:text-gray-600 disabled:cursor-not-allowed`} disabled={isPresetRule} title={isPresetRule ? 'Preset field names cannot be changed' : ''} />
                                    <StyledSelect value={rule.extractFrom} onChange={e => handleRuleChange(rule.id, 'extractFrom', e.target.value as ExtractFrom)} small>
                                        <option value="html-element">HTML Element</option>
                                        <option value="json-ld">JSON-LD</option>
                                        <option value="html-comment">HTML Comment</option>
                                    </StyledSelect>
                                </div>
                                {rule.extractFrom === 'html-element' && (
                                    <>
                                        <div className="flex items-center gap-1">
                                            <input
                                                type="text"
                                                placeholder="CSS Selector"
                                                value={rule.selector || ''}
                                                onChange={e => handleRuleChange(rule.id, 'selector', e.target.value)}
                                                onFocus={() => props.onInspectSelector(rule.selector || null)}
                                                onBlur={() => props.onInspectSelector(null)}
                                                className={`${smallInputClasses} ${props.highlightedSelector === rule.selector ? 'outline outline-2 outline-orange-500' : ''}`}
                                            />
                                            <button
                                                onClick={() => {
                                                    if (props.pickingRuleId === rule.id) {
                                                        props.onStopPicking();
                                                    } else {
                                                        props.onStartPicking(node.id, rule.id);
                                                    }
                                                }}
                                                className={`p-1.5 rounded-md ${props.pickingRuleId === rule.id ? 'bg-blue-600 text-white animate-pulse' : 'bg-gray-200 text-gray-700 hover:bg-gray-300'}`}
                                                title="Pick element from inspector"
                                                disabled={!data.inspectorHtmlContent}
                                            >
                                                <CursorArrowRaysIcon />
                                            </button>
                                        </div>
                                        <StyledSelect value={rule.extract || 'text'} onChange={e => handleRuleChange(rule.id, 'extract', e.target.value)} small>
                                            <option value="text">Extract Text</option>
                                            <option value="attribute">Extract Attribute</option>
                                            <option value="html">Extract HTML</option>
                                            <option value="regex">Extract via Regex</option>
                                        </StyledSelect>
                                        {rule.extract === 'attribute' && <input type="text" placeholder="Attribute Name (e.g., href)" value={rule.attribute || ''} onChange={e => handleRuleChange(rule.id, 'attribute', e.target.value)} className={smallInputClasses} />}
                                        {rule.extract === 'regex' && (
                                            <div className="grid grid-cols-3 gap-2">
                                                <input type="text" placeholder="Regex Pattern" value={rule.regexPattern || ''} onChange={e => handleRuleChange(rule.id, 'regexPattern', e.target.value)} className={`${smallInputClasses} col-span-2`} />
                                                <input type="number" placeholder="Group" value={rule.regexGroup || 0} onChange={e => handleRuleChange(rule.id, 'regexGroup', parseInt(e.target.value))} className={smallInputClasses} />
                                            </div>
                                        )}
                                        <label className="flex items-center gap-2 cursor-pointer select-none mt-1">
                                            <div className="relative" onClick={() => handleRuleChange(rule.id, 'extractMultiple', !rule.extractMultiple)}>
                                                <div className={`w-8 h-4 rounded-full transition-colors ${rule.extractMultiple ? 'bg-teal-500' : 'bg-gray-300'}`} />
                                                <div className={`absolute top-0.5 w-3 h-3 bg-white rounded-full shadow transition-transform ${rule.extractMultiple ? 'translate-x-4' : 'translate-x-0.5'}`} />
                                            </div>
                                            <span className="text-xs text-gray-600">
                                                Extract Multiple
                                                {rule.extractMultiple && <span className="ml-1 text-teal-600 font-semibold">(→ array)</span>}
                                            </span>
                                        </label>
                                    </>
                                )}
                                {(rule.extractFrom === 'json-ld' || rule.extractFrom === 'html-comment') && (
                                    <input type="text" placeholder="JSON Path (e.g., offers.price)" value={rule.jsonPath || ''} onChange={e => handleRuleChange(rule.id, 'jsonPath', e.target.value)} className={smallInputClasses} />
                                )}
                            </div>
                        )
                    })}
                </div>
                <button onClick={addRule} className={`${commonButtonClasses} bg-teal-600 hover:bg-teal-700 mt-3`}>Add Custom Rule</button>
            </CollapsibleSection>
        </div>
    );
};

// --- NEW EXTRACTOR SETTINGS ---

const CSVExtractorSettings: React.FC<{ node: Node<CSVExtractorNodeData>; onUpdate: (data: CSVExtractorNodeData) => void }> = ({ node, onUpdate }) => {
    const { data } = node;

    const handleMappingChange = (id: string, key: keyof ColumnMapping, value: any) => {
        const newMappings = data.mappings.map(m => m.id === id ? { ...m, [key]: value } : m);
        onUpdate({ ...data, mappings: newMappings });
    };
    const addMapping = () => {
        const newMapping: ColumnMapping = { id: `${Date.now()}`, source: String(data.mappings.length), fieldName: `field_${data.mappings.length + 1}` };
        onUpdate({ ...data, mappings: [...data.mappings, newMapping] });
    };
    const removeMapping = (id: string) => {
        onUpdate({ ...data, mappings: data.mappings.filter(m => m.id !== id) });
    };

    const togglePreset = (presetKey: string) => {
        const currentPresets = data.presets || [];
        const preset = PRESETS[presetKey]?.csv;
        if (!preset) return;

        const isCurrentlySelected = currentPresets.includes(presetKey);
        let newMappings = [...data.mappings];

        if (isCurrentlySelected) {
            const presetMappingIds = new Set(preset.mappings.map(m => m.id));
            newMappings = newMappings.filter(m => !presetMappingIds.has(m.id));
            const newPresets = currentPresets.filter(p => p !== presetKey);
            onUpdate({ ...data, presets: newPresets, mappings: newMappings });
        } else {
            const existingMappingIds = new Set(newMappings.map(m => m.id));
            const mappingsToAdd = preset.mappings.filter(m => !existingMappingIds.has(m.id));
            newMappings = [...newMappings, ...mappingsToAdd];
            const newPresets = [...currentPresets, presetKey];
            onUpdate({ ...data, presets: newPresets, mappings: newMappings });
        }
    };

    const presetMappingIds = useMemo(() => {
        const ids = new Set<string>();
        (data.presets || []).forEach(presetKey => {
            PRESETS[presetKey]?.csv?.mappings.forEach(mapping => {
                ids.add(mapping.id);
            });
        });
        return ids;
    }, [data.presets]);

    const availablePresets = useMemo(() => {
        return Object.entries(PRESETS).filter(([_, preset]) => !!preset.csv);
    }, []);

    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">CSV Extractor Settings</h3>
            <div className="flex items-center gap-2">
                <input
                    type="checkbox"
                    id="hasHeader"
                    checked={data.hasHeader}
                    onChange={(e) => onUpdate({ ...data, hasHeader: e.target.checked })}
                    className="h-4 w-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                />
                <label htmlFor="hasHeader" className="text-sm text-gray-700">First row is header</label>
            </div>

            <CollapsibleSection title="Extraction Presets">
                <div className="grid grid-cols-2 gap-2">
                    {availablePresets.map(([key, preset]) => (
                        <div key={key} className="flex items-center gap-2">
                            <input
                                type="checkbox"
                                id={`preset-${key}`}
                                checked={(data.presets || []).includes(key)}
                                onChange={() => togglePreset(key)}
                                className="h-4 w-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                            />
                            <label htmlFor={`preset-${key}`} className="text-sm text-gray-700">{preset.name}</label>
                        </div>
                    ))}
                </div>
            </CollapsibleSection>

            <CollapsibleSection title="Column Mappings" defaultOpen>
                <div className="space-y-2">
                    {data.mappings.map(m => {
                        const isPresetMapping = presetMappingIds.has(m.id);
                        return (
                            <div key={m.id} className={`flex items-center gap-2 p-2 border rounded-md relative ${isPresetMapping ? 'bg-blue-50 border-blue-200' : 'bg-slate-50 border-slate-200'}`}>
                                {isPresetMapping && <span className="absolute top-1 right-2 text-xs font-semibold text-blue-700 bg-blue-200 px-2 py-0.5 rounded-full">Preset</span>}
                                <input type={data.hasHeader ? 'text' : 'number'} placeholder={data.hasHeader ? "Header Name" : "Column Index"} value={m.source} onChange={e => handleMappingChange(m.id, 'source', e.target.value)} className={`${smallInputClasses} disabled:bg-slate-200 disabled:text-gray-600 disabled:cursor-not-allowed`} disabled={isPresetMapping} />
                                <span>-&gt;</span>
                                <input type="text" placeholder="Field Name" value={m.fieldName} onChange={e => handleMappingChange(m.id, 'fieldName', e.target.value)} className={`${smallInputClasses} disabled:bg-slate-200 disabled:text-gray-600 disabled:cursor-not-allowed`} disabled={isPresetMapping} />
                                {!isPresetMapping && <button onClick={() => removeMapping(m.id)} className="text-gray-400 hover:text-red-500"><TrashIcon /></button>}
                            </div>
                        );
                    })}
                </div>
                <button onClick={addMapping} className={`${commonButtonClasses} bg-teal-600 hover:bg-teal-700 mt-3`}>Add Mapping</button>
            </CollapsibleSection>
        </div>
    );
};

const PathBasedExtractorSettings: React.FC<{
    node: Node<JSONExtractorNodeData | XMLExtractorNodeData>;
    onUpdate: (data: JSONExtractorNodeData | XMLExtractorNodeData) => void;
    title: string;
    pathPlaceholder: string;
    presetKey: 'json' | 'xml';
}> = ({ node, onUpdate, title, pathPlaceholder, presetKey }) => {
    const { data } = node;

    const handleMappingChange = (id: string, key: keyof PathMapping, value: any) => {
        const newMappings = data.mappings.map(m => m.id === id ? { ...m, [key]: value } : m);
        onUpdate({ ...data, mappings: newMappings });
    };
    const addMapping = () => {
        const newMapping: PathMapping = { id: `${Date.now()}`, path: '', fieldName: `field_${data.mappings.length + 1}` };
        onUpdate({ ...data, mappings: [...data.mappings, newMapping] });
    };
    const removeMapping = (id: string) => {
        onUpdate({ ...data, mappings: data.mappings.filter(m => m.id !== id) });
    };

    const togglePreset = (key: string) => {
        const currentPresets = data.presets || [];
        const preset = PRESETS[key]?.[presetKey];
        if (!preset) return;

        const isCurrentlySelected = currentPresets.includes(key);
        let newMappings = [...data.mappings];

        if (isCurrentlySelected) {
            const presetMappingIds = new Set(preset.mappings.map(m => m.id));
            newMappings = newMappings.filter(m => !presetMappingIds.has(m.id));
            const newPresets = currentPresets.filter(p => p !== key);
            onUpdate({ ...data, presets: newPresets, mappings: newMappings });
        } else {
            const existingMappingIds = new Set(newMappings.map(m => m.id));
            const mappingsToAdd = preset.mappings.filter(m => !existingMappingIds.has(m.id));
            newMappings = [...newMappings, ...mappingsToAdd];
            const newPresets = [...currentPresets, key];
            onUpdate({ ...data, presets: newPresets, mappings: newMappings });
        }
    };

    const presetMappingIds = useMemo(() => {
        const ids = new Set<string>();
        (data.presets || []).forEach(key => {
            PRESETS[key]?.[presetKey]?.mappings.forEach(mapping => {
                ids.add(mapping.id);
            });
        });
        return ids;
    }, [data.presets, presetKey]);

    const availablePresets = useMemo(() => {
        return Object.entries(PRESETS).filter(([_, preset]) => !!preset[presetKey]);
    }, [presetKey]);

    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">{title}</h3>

            <CollapsibleSection title="Extraction Presets">
                <div className="grid grid-cols-2 gap-2">
                    {availablePresets.map(([key, preset]) => (
                        <div key={key} className="flex items-center gap-2">
                            <input
                                type="checkbox"
                                id={`preset-${key}`}
                                checked={(data.presets || []).includes(key)}
                                onChange={() => togglePreset(key)}
                                className="h-4 w-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                            />
                            <label htmlFor={`preset-${key}`} className="text-sm text-gray-700">{preset.name}</label>
                        </div>
                    ))}
                </div>
            </CollapsibleSection>

            <CollapsibleSection title="Path Mappings" defaultOpen>
                <div className="space-y-2">
                    {data.mappings.map(m => {
                        const isPresetMapping = presetMappingIds.has(m.id);
                        return (
                            <div key={m.id} className={`flex items-center gap-2 p-2 border rounded-md relative ${isPresetMapping ? 'bg-blue-50 border-blue-200' : 'bg-slate-50 border-slate-200'}`}>
                                {isPresetMapping && <span className="absolute top-1 right-2 text-xs font-semibold text-blue-700 bg-blue-200 px-2 py-0.5 rounded-full">Preset</span>}
                                <input type="text" placeholder={pathPlaceholder} value={m.path} onChange={e => handleMappingChange(m.id, 'path', e.target.value)} className={`${smallInputClasses} disabled:bg-slate-200 disabled:text-gray-600 disabled:cursor-not-allowed`} disabled={isPresetMapping} />
                                <span>-&gt;</span>
                                <input type="text" placeholder="Field Name" value={m.fieldName} onChange={e => handleMappingChange(m.id, 'fieldName', e.target.value)} className={`${smallInputClasses} disabled:bg-slate-200 disabled:text-gray-600 disabled:cursor-not-allowed`} disabled={isPresetMapping} />
                                {!isPresetMapping && <button onClick={() => removeMapping(m.id)} className="text-gray-400 hover:text-red-500"><TrashIcon /></button>}
                            </div>
                        );
                    })}
                </div>
                <button onClick={addMapping} className={`${commonButtonClasses} bg-teal-600 hover:bg-teal-700 mt-3`}>Add Mapping</button>
            </CollapsibleSection>
        </div>
    );
};

const MySQLExtractorSettings: React.FC<{ node: Node<MySQLExtractorNodeData>; onUpdate: (data: MySQLExtractorNodeData) => void }> = ({ node, onUpdate }) => {
    const { data } = node;

    const handleMappingChange = (id: string, key: keyof ColumnMapping, value: any) => {
        const newMappings = data.mappings.map(m => m.id === id ? { ...m, [key]: value } : m);
        onUpdate({ ...data, mappings: newMappings });
    };
    const addMapping = () => {
        const newMapping: ColumnMapping = { id: `${Date.now()}`, source: `column_${data.mappings.length + 1}`, fieldName: `field_${data.mappings.length + 1}` };
        onUpdate({ ...data, mappings: [...data.mappings, newMapping] });
    };
    const removeMapping = (id: string) => {
        onUpdate({ ...data, mappings: data.mappings.filter(m => m.id !== id) });
    };

    const togglePreset = (presetKey: string) => {
        const currentPresets = data.presets || [];
        const preset = PRESETS[presetKey]?.mysql;
        if (!preset) return;

        const isCurrentlySelected = currentPresets.includes(presetKey);
        let newMappings = [...data.mappings];

        if (isCurrentlySelected) {
            const presetMappingIds = new Set(preset.mappings.map(m => m.id));
            newMappings = newMappings.filter(m => !presetMappingIds.has(m.id));
            const newPresets = currentPresets.filter(p => p !== presetKey);
            onUpdate({ ...data, presets: newPresets, mappings: newMappings });
        } else {
            const existingMappingIds = new Set(newMappings.map(m => m.id));
            const mappingsToAdd = preset.mappings.filter(m => !existingMappingIds.has(m.id));
            newMappings = [...newMappings, ...mappingsToAdd];
            const newPresets = [...currentPresets, presetKey];
            onUpdate({ ...data, presets: newPresets, mappings: newMappings });
        }
    };

    const presetMappingIds = useMemo(() => {
        const ids = new Set<string>();
        (data.presets || []).forEach(presetKey => {
            PRESETS[presetKey]?.mysql?.mappings.forEach(mapping => {
                ids.add(mapping.id);
            });
        });
        return ids;
    }, [data.presets]);

    const availablePresets = useMemo(() => {
        return Object.entries(PRESETS).filter(([_, preset]) => !!preset.mysql);
    }, []);

    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">MySQL Extractor Settings</h3>

            <CollapsibleSection title="Extraction Presets">
                <div className="grid grid-cols-2 gap-2">
                    {availablePresets.map(([key, preset]) => (
                        <div key={key} className="flex items-center gap-2">
                            <input
                                type="checkbox"
                                id={`preset-${key}`}
                                checked={(data.presets || []).includes(key)}
                                onChange={() => togglePreset(key)}
                                className="h-4 w-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                            />
                            <label htmlFor={`preset-${key}`} className="text-sm text-gray-700">{preset.name}</label>
                        </div>
                    ))}
                </div>
            </CollapsibleSection>

            <CollapsibleSection title="Column Mappings" defaultOpen>
                <div className="space-y-2">
                    {data.mappings.map(m => {
                        const isPresetMapping = presetMappingIds.has(m.id);
                        return (
                            <div key={m.id} className={`flex items-center gap-2 p-2 border rounded-md relative ${isPresetMapping ? 'bg-blue-50 border-blue-200' : 'bg-slate-50 border-slate-200'}`}>
                                {isPresetMapping && <span className="absolute top-1 right-2 text-xs font-semibold text-blue-700 bg-blue-200 px-2 py-0.5 rounded-full">Preset</span>}
                                <input type="text" placeholder="Column Name" value={m.source} onChange={e => handleMappingChange(m.id, 'source', e.target.value)} className={`${smallInputClasses} disabled:bg-slate-200 disabled:text-gray-600 disabled:cursor-not-allowed`} disabled={isPresetMapping} />
                                <span>-&gt;</span>
                                <input type="text" placeholder="Field Name" value={m.fieldName} onChange={e => handleMappingChange(m.id, 'fieldName', e.target.value)} className={`${smallInputClasses} disabled:bg-slate-200 disabled:text-gray-600 disabled:cursor-not-allowed`} disabled={isPresetMapping} />
                                {!isPresetMapping && <button onClick={() => removeMapping(m.id)} className="text-gray-400 hover:text-red-500"><TrashIcon /></button>}
                            </div>
                        );
                    })}
                </div>
                <button onClick={addMapping} className={`${commonButtonClasses} bg-teal-600 hover:bg-teal-700 mt-3`}>Add Mapping</button>
            </CollapsibleSection>
        </div>
    );
};


// --- Processor Node Settings ---
const ProcessorNodeSettings: React.FC<{
    node: Node<ProcessorNodeData>;
    onUpdate: (data: ProcessorNodeData) => void;
    nodes: Node[];
    edges: Edge[];
}> = ({ node, onUpdate, nodes, edges }) => {
    const { data } = node;

    const handleTypeChange = (type: ProcessorNodeData['processorType']) => {
        const processor = PROCESSORS.find(p => p.id === type);
        if (processor) {
            // Fix: Cast strictly to avoid union mismatch issues
            onUpdate({
                processorType: type,
                settings: processor.defaultSettings
            } as unknown as ProcessorNodeData);
        }
    };

    const handleSettingsChange = (key: string, value: any) => {
        const currentSettings = (data as any).settings ?? (data as any).processorConfig ?? {};
        onUpdate({ ...data, settings: { ...currentSettings, [key]: value } } as ProcessorNodeData);
    };
    const getAvailableFields = (startNodeId: string): string[] => {
        const fields = new Set<string>();
        const visited = new Set<string>();
        const queue = [startNodeId];
        const extractorTypes = ['html-data-extractor', 'csv-extractor', 'json-extractor', 'xml-extractor', 'mysql-extractor'];

        while (queue.length > 0) {
            const currentId = queue.shift()!;
            if (visited.has(currentId)) continue;
            visited.add(currentId);

            const currentNode = nodes.find(n => n.id === currentId);
            if (!currentNode) continue;

            if (extractorTypes.includes(currentNode.type || '')) {
                if (currentNode.type === 'html-data-extractor') {
                    const d = currentNode.data as HTMLDataExtractorNodeData;
                    d.customRules.forEach(r => fields.add(r.name));
                    // Add preset fields
                    (d.presets || []).forEach(presetKey => {
                        PRESETS[presetKey as string]?.html?.rules.forEach(r => fields.add(r.name));
                    });
                } else if (currentNode.type === 'csv-extractor') {
                    const d = currentNode.data as CSVExtractorNodeData;
                    d.mappings.forEach(m => fields.add(m.fieldName));
                    (d.presets || []).forEach(presetKey => {
                        PRESETS[presetKey as string]?.csv?.mappings.forEach(m => fields.add(m.fieldName));
                    });
                } else if (currentNode.type === 'json-extractor') {
                    const d = currentNode.data as JSONExtractorNodeData;
                    d.mappings.forEach(m => fields.add(m.fieldName));
                    (d.presets || []).forEach(presetKey => {
                        PRESETS[presetKey as string]?.json?.mappings.forEach(m => fields.add(m.fieldName));
                    });
                } else if (currentNode.type === 'xml-extractor') {
                    const d = currentNode.data as XMLExtractorNodeData;
                    d.mappings.forEach(m => fields.add(m.fieldName));
                    (d.presets || []).forEach(presetKey => {
                        PRESETS[presetKey as string]?.xml?.mappings.forEach(m => fields.add(m.fieldName));
                    });
                } else if (currentNode.type === 'mysql-extractor') {
                    const d = currentNode.data as MySQLExtractorNodeData;
                    d.mappings.forEach(m => fields.add(m.fieldName));
                    (d.presets || []).forEach(presetKey => {
                        PRESETS[presetKey as string]?.mysql?.mappings.forEach(m => fields.add(m.fieldName));
                    });
                }
                // Once we hit an extractor, we usually stop this branch, as this is the source of truth for fields
                continue;
            }

            // Find incoming edges to traverse upstream
            const incomingEdges = edges.filter(e => e.target === currentId);
            incomingEdges.forEach(e => queue.push(e.source));
        }
        return Array.from(fields);
    };

    const availableFields = useMemo(() => getAvailableFields(node.id), [nodes, edges, node.id]);
    const updateMapping = (field: string, value: string, mappingKey: string) => {
        const currentSettings = (data.settings ?? (data as any).processorConfig ?? {}) as any;
        const newMapping = { ...(currentSettings[mappingKey] || {}), [field]: value };
        handleSettingsChange(mappingKey, newMapping);
    };

    const renderMappingSection = (
        autoMapKey: string,
        mappingKey: string,
        destLabel: string,
        autoMapLabel: string = "Auto Map Fields"
    ) => {
        const settings = (data.settings ?? (data as any).processorConfig ?? {}) as any;
        const isAutoMap = settings[autoMapKey];

        return (
            <div className="pt-2 border-t mt-2">
                <label className="flex items-center gap-2 mb-2">
                    <input
                        type="checkbox"
                        checked={isAutoMap || false}
                        onChange={e => handleSettingsChange(autoMapKey, e.target.checked)}
                        className="h-4 w-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                    />
                    <span className="text-sm font-semibold text-gray-700">{autoMapLabel}</span>
                </label>

                <div className="space-y-2 bg-slate-50 p-2 rounded border border-slate-200">
                    <div className="text-xs font-bold text-gray-500 uppercase flex justify-between px-1">
                        <span>Extracted Field</span>
                        <span>{destLabel}</span>
                    </div>
                    {availableFields.length === 0 ? (
                        <div className="text-xs text-gray-400 italic text-center py-2">No fields found from upstream extractors.</div>
                    ) : (
                        availableFields.map(field => (
                            <div key={field} className="flex items-center gap-2">
                                <div className="flex-1 text-sm bg-white border border-gray-200 px-2 py-1.5 rounded text-gray-700 truncate" title={field}>
                                    {field}
                                </div>
                                <span className="text-gray-400">→</span>
                                <input
                                    type="text"
                                    placeholder={field}
                                    value={isAutoMap ? field : ((settings[mappingKey] || {})[field] || '')}
                                    onChange={e => updateMapping(field, e.target.value, mappingKey)}
                                    disabled={isAutoMap}
                                    className={`${smallInputClasses} flex-1`}
                                />
                            </div>
                        ))
                    )}
                </div>
            </div>
        );
    };

    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">Processor Settings</h3>
            <div>
                <label className={commonLabelClasses}>Processor Type</label>
                <StyledSelect value={data.processorType} onChange={e => handleTypeChange(e.target.value as any)}>
                    {PROCESSORS.map(p => (
                        <option key={p.id} value={p.id}>{p.name}</option>
                    ))}
                </StyledSelect>
            </div>

            <CollapsibleSection title="Configuration" defaultOpen>
                <div className="space-y-3">
                    {(() => {
                        const s: any = data.settings ?? (data as any).processorConfig ?? {};

                        return (
                            <>
                                {data.processorType === 'save-to-database' && (
                                    <>
                                        <StyledSelect value={s.connectionType || 'mysql'} onChange={e => handleSettingsChange('connectionType', e.target.value)}>
                                            <option value="mysql">MySQL</option>
                                            <option value="postgresql">PostgreSQL</option>
                                        </StyledSelect>
                                        <input type="text" placeholder="Host" value={s.host || ''} onChange={e => handleSettingsChange('host', e.target.value)} className={commonInputClasses} />
                                        <input type="text" placeholder="User" value={s.user || ''} onChange={e => handleSettingsChange('user', e.target.value)} className={commonInputClasses} />
                                        <input type="password" placeholder="Password" value={s.password || ''} onChange={e => handleSettingsChange('password', e.target.value)} className={commonInputClasses} />
                                        <input type="text" placeholder="Database" value={s.database || ''} onChange={e => handleSettingsChange('database', e.target.value)} className={commonInputClasses} />
                                        <input type="text" placeholder="Table Name" value={s.tableName || ''} onChange={e => handleSettingsChange('tableName', e.target.value)} className={commonInputClasses} />
                                        <StyledSelect value={s.conflictStrategy || 'insert'} onChange={e => handleSettingsChange('conflictStrategy', e.target.value)}>
                                            <option value="insert">Insert (Fail on Duplicate)</option>
                                            <option value="upsert">Upsert (Update on Duplicate)</option>
                                            <option value="skip">Skip on Duplicate</option>
                                        </StyledSelect>

                                        {renderMappingSection('autoMapColumns', 'columnMapping', 'DB Column')}
                                    </>
                                )}
                                {data.processorType === 'send-to-api' && (
                                    <>
                                        <input type="url" placeholder="Endpoint URL" value={s.endpointUrl || ''} onChange={e => handleSettingsChange('endpointUrl', e.target.value)} className={commonInputClasses} />
                                        <StyledSelect value={s.method || 'POST'} onChange={e => handleSettingsChange('method', e.target.value)}>
                                            <option value="POST">POST</option>
                                            <option value="PUT">PUT</option>
                                            <option value="PATCH">PATCH</option>
                                        </StyledSelect>

                                        {renderMappingSection('autoMapFields', 'fieldMapping', 'JSON Key')}
                                    </>
                                )}
                                {data.processorType === 'generate-csv-file' && (
                                    <>
                                        <input type="text" placeholder="File Name Pattern" value={s.fileName || ''} onChange={e => handleSettingsChange('fileName', e.target.value)} className={commonInputClasses} />
                                        <StyledSelect value={s.delimiter || ','} onChange={e => handleSettingsChange('delimiter', e.target.value)}>
                                            <option value=",">Comma (,)</option>
                                            <option value=";">Semicolon (;)</option>
                                            <option value="\t">Tab (\t)</option>
                                        </StyledSelect>
                                        <div className="flex items-center gap-2 mt-2">
                                            <input
                                                type="checkbox"
                                                id="includeHeader"
                                                checked={!!s.includeHeader}
                                                onChange={e => handleSettingsChange('includeHeader', e.target.checked)}
                                                className="h-4 w-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                                            />
                                            <label htmlFor="includeHeader" className="text-sm text-gray-700">Include Header Row</label>
                                        </div>

                                        {renderMappingSection('autoMapHeaders', 'columnMapping', 'CSV Header')}
                                    </>
                                )}
                                {data.processorType === 'send-email-notification' && (
                                    <>
                                        <input type="text" placeholder="Recipients (comma separated)" value={s.recipients || ''} onChange={e => handleSettingsChange('recipients', e.target.value)} className={commonInputClasses} />
                                        <input type="text" placeholder="Subject" value={s.subject || ''} onChange={e => handleSettingsChange('subject', e.target.value)} className={commonInputClasses} />
                                        <textarea placeholder="Body Template" value={s.body || ''} onChange={e => handleSettingsChange('body', e.target.value)} className={`${commonInputClasses} h-24`} />

                                        {renderMappingSection('autoMapFields', 'fieldMapping', 'Label in Email')}
                                    </>
                                )}
                                {data.processorType === 'generate-excel-file' && (
                                    <>
                                        <input type="text" placeholder="File Name Pattern" value={s.fileName || ''} onChange={e => handleSettingsChange('fileName', e.target.value)} className={commonInputClasses} />
                                        <input type="text" placeholder="Sheet Name" value={s.sheetName || ''} onChange={e => handleSettingsChange('sheetName', e.target.value)} className={commonInputClasses} />
                                        <div className="flex items-center gap-2 mt-2">
                                            <input
                                                type="checkbox"
                                                id="includeHeaderExcel"
                                                checked={!!s.includeHeader}
                                                onChange={e => handleSettingsChange('includeHeader', e.target.checked)}
                                                className="h-4 w-4 rounded border-slate-300 text-blue-600 focus:ring-blue-500"
                                            />
                                            <label htmlFor="includeHeaderExcel" className="text-sm text-gray-700">Include Header Row</label>
                                        </div>

                                        {renderMappingSection('autoMapHeaders', 'columnMapping', 'Excel Column')}
                                    </>
                                )}
                            </>
                        );
                    })()}
                </div>
            </CollapsibleSection>
        </div>
    );
};

const WorkerNodeSettings: React.FC<{ node: Node<WorkerNodeData>; onUpdate: (data: WorkerNodeData) => void }> = ({ node, onUpdate }) => {
    const { data } = node;

    const addRule = () => {
        const newRule: WorkerRule = { id: `${Date.now()}`, type: 'dom-value', selector: '', condition: 'exists', value: '' };
        onUpdate({ ...data, detectionRules: [...(data.detectionRules || []), newRule] });
    };

    const updateRule = (id: string, ruleUpdate: Partial<WorkerRule>) => {
        const newRules = (data.detectionRules || []).map(r => r.id === id ? { ...r, ...ruleUpdate } as WorkerRule : r);
        onUpdate({ ...data, detectionRules: newRules });
    };

    const removeRule = (id: string) => {
        onUpdate({ ...data, detectionRules: (data.detectionRules || []).filter(r => r.id !== id) });
    };

    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">Worker Settings</h3>
            <div className="flex justify-between items-center">
                <label className={commonLabelClasses}>Priority</label>
                <input type="number" value={data.priority || 0} onChange={e => onUpdate({ ...data, priority: parseInt(e.target.value) || 0 })} className={`${commonInputClasses} w-24`} />
            </div>

            <CollapsibleSection title="Detection Rules" defaultOpen>
                <div className="space-y-4">
                    <div className="flex items-center gap-2 mb-2 p-3 bg-slate-50 rounded border border-slate-200">
                        <span className="text-sm text-gray-700 font-medium">Match:</span>
                        <StyledSelect value={data.detectionLogic || 'and'} onChange={e => onUpdate({ ...data, detectionLogic: e.target.value as 'and' | 'or' })} className="w-32">
                            <option value="and">ALL</option>
                            <option value="or">ANY</option>
                        </StyledSelect>
                        <span className="text-sm text-gray-700">of the following rules:</span>
                    </div>

                    {(data.detectionRules || []).map((rule, index) => (
                        <div key={rule.id} className="p-4 border border-slate-200 bg-white rounded-lg shadow-sm space-y-4 relative group transition-all hover:border-blue-300">

                            <div className="flex justify-between items-center border-b border-slate-100 pb-2 mb-2">
                                <span className="text-xs font-bold text-gray-400 uppercase tracking-wider">Rule {index + 1}</span>
                                <button
                                    onClick={() => removeRule(rule.id)}
                                    className="text-gray-400 hover:text-red-500 p-1.5 hover:bg-red-50 rounded transition-colors"
                                    title="Remove Rule"
                                >
                                    <TrashIcon />
                                </button>
                            </div>

                            <div className="space-y-4">
                                <div>
                                    <label className={commonLabelClasses}>Rule Type</label>
                                    <StyledSelect value={rule.type} onChange={e => updateRule(rule.id, { type: e.target.value as WorkerRuleType })}>
                                        <option value="url-format">URL Format</option>
                                        <option value="html-contains">HTML Contains</option>
                                        <option value="dom-value">DOM Element Value</option>
                                        <option value="tag-attribute">Tag Attribute</option>
                                        <option value="data-source-type">Data Source Type</option>
                                    </StyledSelect>
                                </div>

                                {rule.type === 'url-format' && (
                                    <div>
                                        <label className={commonLabelClasses}>Regex Pattern</label>
                                        <input type="text" placeholder="e.g., /products/.*" value={(rule as URLFormatRule).pattern} onChange={e => updateRule(rule.id, { pattern: e.target.value })} className={commonInputClasses} />
                                    </div>
                                )}
                                {rule.type === 'html-contains' && (
                                    <div>
                                        <label className={commonLabelClasses}>Text Content</label>
                                        <input type="text" placeholder="Text to match" value={(rule as HTMLContainsRule).text} onChange={e => updateRule(rule.id, { text: e.target.value })} className={commonInputClasses} />
                                    </div>
                                )}
                                {rule.type === 'dom-value' && (
                                    <>
                                        <div>
                                            <label className={commonLabelClasses}>CSS Selector</label>
                                            <input type="text" placeholder="e.g., .price" value={(rule as DOMValueRule).selector} onChange={e => updateRule(rule.id, { selector: e.target.value })} className={commonInputClasses} />
                                        </div>
                                        <div className="grid grid-cols-2 gap-3">
                                            <div>
                                                <label className={commonLabelClasses}>Condition</label>
                                                <StyledSelect value={(rule as DOMValueRule).condition} onChange={e => updateRule(rule.id, { condition: e.target.value as RuleCondition })}>
                                                    <option value="exists">Exists</option>
                                                    <option value="not-exists">Not Exists</option>
                                                    <option value="contains">Contains</option>
                                                    <option value="not-contains">Not Contains</option>
                                                    <option value="matches-regex">Matches Regex</option>
                                                </StyledSelect>
                                            </div>
                                            {['contains', 'not-contains', 'matches-regex'].includes((rule as DOMValueRule).condition) && (
                                                <div>
                                                    <label className={commonLabelClasses}>Value</label>
                                                    <input type="text" placeholder="Value to check" value={(rule as DOMValueRule).value} onChange={e => updateRule(rule.id, { value: e.target.value })} className={commonInputClasses} />
                                                </div>
                                            )}
                                        </div>
                                    </>
                                )}
                                {rule.type === 'tag-attribute' && (
                                    <>
                                        <div>
                                            <label className={commonLabelClasses}>CSS Selector</label>
                                            <input type="text" placeholder="e.g., a.link" value={(rule as TagAttributeRule).selector} onChange={e => updateRule(rule.id, { selector: e.target.value })} className={commonInputClasses} />
                                        </div>
                                        <div>
                                            <label className={commonLabelClasses}>Attribute</label>
                                            <input type="text" placeholder="e.g., href" value={(rule as TagAttributeRule).attribute} onChange={e => updateRule(rule.id, { attribute: e.target.value })} className={commonInputClasses} />
                                        </div>
                                        <div className="grid grid-cols-2 gap-3">
                                            <div>
                                                <label className={commonLabelClasses}>Condition</label>
                                                <StyledSelect value={(rule as TagAttributeRule).condition} onChange={e => updateRule(rule.id, { condition: e.target.value as RuleCondition })}>
                                                    <option value="exists">Exists</option>
                                                    <option value="not-exists">Not Exists</option>
                                                    <option value="contains">Contains</option>
                                                    <option value="not-contains">Not Contains</option>
                                                    <option value="matches-regex">Matches Regex</option>
                                                </StyledSelect>
                                            </div>
                                            {['contains', 'not-contains', 'matches-regex'].includes((rule as TagAttributeRule).condition) && (
                                                <div>
                                                    <label className={commonLabelClasses}>Value</label>
                                                    <input type="text" placeholder="Value to check" value={(rule as TagAttributeRule).value} onChange={e => updateRule(rule.id, { value: e.target.value })} className={commonInputClasses} />
                                                </div>
                                            )}
                                        </div>
                                    </>
                                )}
                                {rule.type === 'data-source-type' && (
                                    <div>
                                        <label className={commonLabelClasses}>Source Type</label>
                                        <StyledSelect value={(rule as DataSourceTypeRule).sourceType || 'url'} onChange={e => updateRule(rule.id, { sourceType: e.target.value } as any)}>
                                            <option value="url">URL</option>
                                            <option value="api">API</option>
                                            <option value="xml">XML</option>
                                            <option value="csv">CSV</option>
                                            <option value="json">JSON</option>
                                            <option value="mysql">MySQL</option>
                                        </StyledSelect>
                                    </div>
                                )}
                            </div>
                        </div>
                    ))}
                    <button onClick={addRule} className={`${commonButtonClasses} bg-purple-600 hover:bg-purple-700 mt-4 py-3 shadow-sm`}>+ Add Detection Rule</button>
                </div>
            </CollapsibleSection>
        </div>
    )
};

const ShapeNodeSettings: React.FC<{ node: Node<ShapeNodeData>; onUpdate: (data: ShapeNodeData) => void }> = ({ node, onUpdate }) => {
    const { data } = node;

    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">Shape Settings</h3>
            <div>
                <label className={commonLabelClasses}>Label</label>
                <input type="text" value={data.label} onChange={e => onUpdate({ ...data, label: e.target.value })} className={commonInputClasses} />
            </div>
            <div className="grid grid-cols-2 gap-4">
                <div>
                    <label className={commonLabelClasses}>Background Color</label>
                    <input type="color" value={data.backgroundColor} onChange={e => onUpdate({ ...data, backgroundColor: e.target.value })} className="w-full h-10 p-1 rounded-md cursor-pointer border border-gray-300" />
                </div>
                <div>
                    <label className={commonLabelClasses}>Border Color</label>
                    <input type="color" value={data.borderColor} onChange={e => onUpdate({ ...data, borderColor: e.target.value })} className="w-full h-10 p-1 rounded-md cursor-pointer border border-gray-300" />
                </div>
                <div>
                    <label className={commonLabelClasses}>Text Color</label>
                    <input type="color" value={data.textColor} onChange={e => onUpdate({ ...data, textColor: e.target.value })} className="w-full h-10 p-1 rounded-md cursor-pointer border border-gray-300" />
                </div>
            </div>
        </div>
    );
};


const PREPROCESSOR_SCHEMA: SettingsSchema = {
    type: 'object',
    properties: {
        inputType: {
            key: 'inputType',
            title: 'Input Type',
            type: 'select',
            required: true,
            order: 1,
            options: [
                { value: 'html', label: 'HTML' },
                { value: 'csv', label: 'CSV' },
                { value: 'json', label: 'JSON' },
                { value: 'xml', label: 'XML' },
                { value: 'text', label: 'Text' },
            ],
            default: 'html',
        },
        itemSelector: {
            key: 'itemSelector',
            title: 'CSS Item Selector',
            type: 'string',
            order: 2,
            description: 'CSS selector for item elements (e.g., "div.product")',
            placeholder: 'e.g., div.product',
            conditions: [{ field: 'inputType', operator: 'eq', value: 'html' }],
        },
        csvDelimiter: {
            key: 'csvDelimiter',
            title: 'CSV Delimiter',
            type: 'string',
            order: 3,
            default: ',',
            placeholder: ',',
            conditions: [{ field: 'inputType', operator: 'eq', value: 'csv' }],
        },
        csvHasHeader: {
            key: 'csvHasHeader',
            title: 'CSV Has Header',
            type: 'boolean',
            order: 4,
            default: true,
            conditions: [{ field: 'inputType', operator: 'eq', value: 'csv' }],
        },
        jsonItemPath: {
            key: 'jsonItemPath',
            title: 'JSON Item Path',
            type: 'string',
            order: 5,
            description: 'JSON path to items array (e.g., "$.data.items")',
            placeholder: 'e.g., $.data.items',
            conditions: [{ field: 'inputType', operator: 'eq', value: 'json' }],
        },
        urlPatterns: {
            key: 'urlPatterns',
            title: 'URL Patterns',
            type: 'array',
            order: 6,
            description: 'Patterns to match extracted URLs against',
            item_field: {
                key: 'urlPattern',
                title: 'URL Pattern',
                type: 'group',
                order: 0,
                fields: [
                    { key: 'enabled', title: 'Enabled', type: 'boolean', order: 1, default: true },
                    {
                        key: 'type', title: 'Pattern Type', type: 'select', order: 2, default: 'contains',
                        options: [
                            { value: 'wildcard', label: 'Wildcard' },
                            { value: 'regex', label: 'Regex' },
                            { value: 'contains', label: 'Contains' },
                            { value: 'startswith', label: 'Starts With' },
                            { value: 'endswith', label: 'Ends With' },
                            { value: 'always', label: 'Always Match' },
                        ],
                    },
                    { key: 'value', title: 'Pattern Value', type: 'string', order: 3, placeholder: 'e.g., https://example.com/*' },
                ],
            },
        },
        extractRules: {
            key: 'extractRules',
            title: 'Extract Rules',
            type: 'array',
            order: 7,
            description: 'Rules for extracting specific fields from each item',
            item_field: {
                key: 'extractRule',
                title: 'Extract Rule',
                type: 'group',
                order: 0,
                fields: [
                    { key: 'type', title: 'Rule Type', type: 'string', order: 1, placeholder: 'e.g., attribute, text, html' },
                    { key: 'value', title: 'Selector / Value', type: 'string', order: 2, placeholder: 'CSS selector or attribute name' },
                    { key: 'attribute', title: 'Attribute', type: 'string', order: 3, placeholder: 'e.g., href, src (optional)' },
                ],
            },
        },
    },
};

const PreprocessorNodeSettings: React.FC<{ node: Node<PreprocessorNodeData>; onUpdate: (data: PreprocessorNodeData) => void }> = ({ node, onUpdate }) => {
    const { data } = node;

    const values = useMemo<Record<string, unknown>>(() => ({
        inputType: data.inputType,
        itemSelector: data.itemSelector ?? '',
        csvDelimiter: data.csvDelimiter ?? ',',
        csvHasHeader: data.csvHasHeader ?? true,
        jsonItemPath: data.jsonItemPath ?? '',
        urlPatterns: data.urlPatterns ?? [],
        extractRules: data.extractRules ?? [],
    }), [data]);

    const handleChange = (newValues: Record<string, unknown>) => {
        onUpdate({
            inputType: newValues.inputType as PreprocessorNodeData['inputType'],
            itemSelector: newValues.itemSelector as string | undefined,
            csvDelimiter: newValues.csvDelimiter as string | undefined,
            csvHasHeader: newValues.csvHasHeader as boolean | undefined,
            jsonItemPath: newValues.jsonItemPath as string | undefined,
            urlPatterns: (newValues.urlPatterns ?? []) as PreprocessorNodeData['urlPatterns'],
            extractRules: (newValues.extractRules ?? []) as PreprocessorNodeData['extractRules'],
        });
    };

    return (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">Preprocessor Settings</h3>
            <DynamicForm
                schema={PREPROCESSOR_SCHEMA}
                values={values}
                onChange={handleChange}
            />
        </div>
    );
};

// --- Main Settings Panel Component ---
const SettingsPanel: React.FC<SettingsPanelProps> = (props) => {
    const { node, onUpdateNode, onDeleteNode, onClose, projectSettings, onUpdateProjectSettings, onExport, onSave, onImport, isOpen, nodes = [], edges = [], isRunning, serviceStatus, serviceCycleCount } = props;

    const renderNodeSettings = () => {
        if (!node) return null;

        const handleUpdate = (data: NodeData) => onUpdateNode(node.id, data);

        switch (node.type) {
            case 'start':
                return <StartNodeSettings node={node as Node<StartNodeData>} onUpdate={handleUpdate as (data: StartNodeData) => void} />;
            case 'click':
                return <ClickNodeSettings node={node as Node<ClickNodeData>} onUpdate={handleUpdate as (data: ClickNodeData) => void} />;
            case 'loop':
                return <LoopNodeSettings node={node as Node<LoopNodeData>} onUpdate={handleUpdate as (data: LoopNodeData) => void} />;
            case 'worker':
                return <WorkerNodeSettings node={node as Node<WorkerNodeData>} onUpdate={handleUpdate as (data: WorkerNodeData) => void} />;
            case 'html-data-extractor':
                return <HTMLDataExtractorSettings node={node as Node<HTMLDataExtractorNodeData>} onUpdate={handleUpdate as (data: HTMLDataExtractorNodeData) => void} props={props} />;
            case 'csv-extractor':
                return <CSVExtractorSettings node={node as Node<CSVExtractorNodeData>} onUpdate={handleUpdate as (data: CSVExtractorNodeData) => void} />;
            case 'json-extractor':
                return <PathBasedExtractorSettings node={node as Node<JSONExtractorNodeData>} onUpdate={handleUpdate as (data: JSONExtractorNodeData) => void} title="JSON Data Extractor" pathPlaceholder="JSON Path (e.g., $.items[0].name)" presetKey="json" />;
            case 'xml-extractor':
                return <PathBasedExtractorSettings node={node as Node<XMLExtractorNodeData>} onUpdate={handleUpdate as (data: XMLExtractorNodeData) => void} title="XML Data Extractor" pathPlaceholder="XPath (e.g., /root/item/name)" presetKey="xml" />;
            case 'mysql-extractor':
                return <MySQLExtractorSettings node={node as Node<MySQLExtractorNodeData>} onUpdate={handleUpdate as (data: MySQLExtractorNodeData) => void} />;
            case 'processor':
                return <ProcessorNodeSettings node={node as Node<ProcessorNodeData>} onUpdate={handleUpdate as (data: ProcessorNodeData) => void} nodes={nodes} edges={edges} />;
            case 'preprocessor':
                return <PreprocessorNodeSettings node={node as Node<PreprocessorNodeData>} onUpdate={handleUpdate as (data: PreprocessorNodeData) => void} />;
            case 'shape':
                return <ShapeNodeSettings node={node as Node<ShapeNodeData>} onUpdate={handleUpdate as (data: ShapeNodeData) => void} />;
            case 'repository':
                return <div className="text-gray-500 italic text-center p-4">This node holds the data. No specific settings available.</div>;
            case 'reception':
                return <div className="text-gray-500 italic text-center p-4">Configuration for Reception is handled via logic rules. (Coming Soon)</div>;
            case 'completion':
                return <div className="text-gray-500 italic text-center p-4">End of workflow. Reporting settings are global.</div>;
            default:
                return <div className="text-gray-500 italic text-center p-4">No settings available for this node type.</div>;
        }
    };

    const renderProjectSettings = () => (
        <div className="space-y-4">
            <h3 className="text-lg font-bold text-gray-800 border-b pb-2">Project Settings</h3>

            {props.projectId && (
                <ServiceControls
                    projectId={props.projectId}
                    onOpenLogs={() => props.onOpenLogs?.()}
                    nodes={nodes}
                    edges={edges}
                    externalStatus={serviceStatus}
                    externalCycleCount={serviceCycleCount}
                />
            )}

            <CollapsibleSection title="General" defaultOpen>
                <div className="flex items-center justify-between mb-4 bg-slate-50 p-3 rounded-lg border border-slate-200">
                    <div className="flex flex-col">
                        <span className="text-sm font-bold text-gray-700">Enable Project</span>
                        <span className="text-xs text-gray-500">{projectSettings.enabled ? 'Project is active' : 'Project is disabled'}</span>
                    </div>
                    <button
                        type="button"
                        onClick={() => onUpdateProjectSettings({ enabled: !projectSettings.enabled })}
                        disabled={isRunning}
                        className={`relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-50 ${projectSettings.enabled ? 'bg-green-500' : 'bg-gray-300'}`}
                        role="switch"
                        aria-checked={projectSettings.enabled}
                    >
                        <span
                            aria-hidden="true"
                            className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${projectSettings.enabled ? 'translate-x-5' : 'translate-x-0'}`}
                        />
                    </button>
                </div>

                <div>
                    <label className={commonLabelClasses}>Project Name</label>
                    <input type="text" value={projectSettings.name} onChange={e => onUpdateProjectSettings({ name: e.target.value })} className={commonInputClasses} disabled={isRunning} />
                </div>
                <div>
                    <label className={commonLabelClasses}>Description</label>
                    <textarea value={projectSettings.description} onChange={e => onUpdateProjectSettings({ description: e.target.value })} className={`${commonInputClasses} h-24`} disabled={isRunning} />
                </div>
                <div className="grid grid-cols-2 gap-4">
                    <div>
                        <label className={commonLabelClasses}>Crawl Delay (ms)</label>
                        <input type="number" value={projectSettings.crawlDelay} onChange={e => onUpdateProjectSettings({ crawlDelay: parseInt(e.target.value) })} className={commonInputClasses} disabled={isRunning} />
                    </div>
                    <div>
                        <label className={commonLabelClasses}>Concurrency</label>
                        <input type="number" value={projectSettings.concurrency} onChange={e => onUpdateProjectSettings({ concurrency: parseInt(e.target.value) })} className={commonInputClasses} disabled={isRunning} />
                    </div>
                </div>
                <div>
                    <label className={commonLabelClasses}>Execution Mode</label>
                    <div className="flex gap-2">
                        <button
                            type="button"
                            onClick={() => onUpdateProjectSettings({ executionMode: 'queue' })}
                            disabled={isRunning}
                            className={`flex-1 p-2 rounded-md text-sm font-semibold border transition-colors ${projectSettings.executionMode === 'queue' ? 'bg-blue-600 text-white border-blue-600' : 'bg-white text-gray-700 border-gray-300 hover:bg-gray-50'} disabled:opacity-50`}
                        >
                            Queue
                        </button>
                        <button
                            type="button"
                            onClick={() => onUpdateProjectSettings({ executionMode: 'parallel' })}
                            disabled={isRunning}
                            className={`flex-1 p-2 rounded-md text-sm font-semibold border transition-colors ${projectSettings.executionMode === 'parallel' ? 'bg-blue-600 text-white border-blue-600' : 'bg-white text-gray-700 border-gray-300 hover:bg-gray-50'} disabled:opacity-50`}
                        >
                            Parallel
                        </button>
                    </div>
                    <p className="text-xs text-gray-500 mt-1">
                        {projectSettings.executionMode === 'queue'
                            ? 'Nodes run one at a time in order.'
                            : `Nodes at the same level run concurrently (max ${projectSettings.concurrency}).`}
                    </p>
                </div>
                <div>
                    <label className={commonLabelClasses}>User Agent</label>
                    <input type="text" value={projectSettings.userAgent} onChange={e => onUpdateProjectSettings({ userAgent: e.target.value })} className={commonInputClasses} disabled={isRunning} />
                </div>

            </CollapsibleSection>

            <CollapsibleSection title="Export">
                <div className="flex items-center justify-between mb-4 bg-slate-50 p-3 rounded-lg border border-slate-200">
                    <div className="flex flex-col">
                        <span className="text-sm font-bold text-gray-700">Group export by project</span>
                        <span className="text-xs text-gray-500">Put exported files in a per-project subfolder (file name also includes the project id).</span>
                    </div>
                    <button
                        type="button"
                        onClick={() => onUpdateProjectSettings({ groupExport: !projectSettings.groupExport })}
                        disabled={isRunning}
                        className={`relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 disabled:opacity-50 ${projectSettings.groupExport ? 'bg-green-500' : 'bg-gray-300'}`}
                        role="switch"
                        aria-checked={projectSettings.groupExport}
                    >
                        <span
                            aria-hidden="true"
                            className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${projectSettings.groupExport ? 'translate-x-5' : 'translate-x-0'}`}
                        />
                    </button>
                </div>
                <div>
                    <label className={commonLabelClasses}>Subfolder name format</label>
                    <div className="flex gap-2">
                        {(['id', 'name', 'both'] as const).map((fmt) => (
                            <button
                                key={fmt}
                                type="button"
                                onClick={() => onUpdateProjectSettings({ groupFormat: fmt })}
                                disabled={isRunning || !projectSettings.groupExport}
                                className={`flex-1 p-2 rounded-md text-sm font-semibold border transition-colors disabled:opacity-50 ${projectSettings.groupFormat === fmt ? 'bg-blue-600 text-white border-blue-600' : 'bg-white text-gray-700 border-gray-300 hover:bg-gray-50'}`}
                            >
                                {fmt === 'id' ? 'Project ID' : fmt === 'name' ? 'Project Name' : 'ID + Name'}
                            </button>
                        ))}
                    </div>
                    <p className="text-xs text-gray-500 mt-1">Used only when grouping is enabled. The export folder itself is set in the global App Settings.</p>
                </div>
            </CollapsibleSection>

            <CollapsibleSection title="Refresh">
                <div className="mb-4">
                    <label className={commonLabelClasses}>Strategy</label>
                    <div className="flex flex-col gap-2">
                        {([
                            { value: 'refresh', label: 'Refresh', desc: 'Re-crawl everything every cycle' },
                            { value: 'refresh_update', label: 'Refresh + Update', desc: 'Re-crawl, process only new items' },
                            { value: 'update_only', label: 'Update Only', desc: 'Check for new data, then stop' },
                        ] as const).map((opt) => (
                            <button
                                key={opt.value}
                                type="button"
                                onClick={() => onUpdateProjectSettings({ refreshStrategy: opt.value })}
                                disabled={isRunning}
                                className={`text-left p-3 rounded-md border transition-colors disabled:opacity-50 ${
                                    (projectSettings.refreshStrategy || 'update_only') === opt.value
                                        ? 'bg-blue-600 text-white border-blue-600'
                                        : 'bg-white text-gray-700 border-gray-300 hover:bg-gray-50'
                                }`}
                            >
                                <div className="font-semibold text-sm">{opt.label}</div>
                                <div className="text-xs mt-0.5 opacity-80">{opt.desc}</div>
                            </button>
                        ))}
                    </div>
                </div>

                {(projectSettings.refreshStrategy || 'update_only') === 'update_only' && (
                    <div className="mb-4">
                        <label className={commonLabelClasses}>Update Method</label>
                        <div className="flex gap-2">
                            {([
                                { value: 'check_first_page_until_duplicate', label: 'From page 1', desc: 'Scan from page 1 until hitting duplicates' },
                                { value: 'check_last_page', label: 'From last page', desc: 'Continue from last crawled page' },
                            ] as const).map((opt) => (
                                <button
                                    key={opt.value}
                                    type="button"
                                    onClick={() => onUpdateProjectSettings({ updateMethod: opt.value })}
                                    disabled={isRunning}
                                    className={`flex-1 p-2 rounded-md text-sm font-semibold border transition-colors disabled:opacity-50 ${
                                        (projectSettings.updateMethod || 'check_first_page_until_duplicate') === opt.value
                                            ? 'bg-blue-600 text-white border-blue-600'
                                            : 'bg-white text-gray-700 border-gray-300 hover:bg-gray-50'
                                    }`}
                                >
                                    {opt.label}
                                </button>
                            ))}
                        </div>
                        <p className="text-xs text-gray-500 mt-1">
                            {(projectSettings.updateMethod || 'check_first_page_until_duplicate') === 'check_last_page'
                                ? 'Start from the last page + 1 and go forward.'
                                : 'Scan from page 1, stop when all URLs on a page already exist in DB.'}
                        </p>
                    </div>
                )}

                {(projectSettings.refreshStrategy || 'update_only') !== 'update_only' && (
                    <div>
                        <label className={commonLabelClasses}>Refresh Interval (seconds)</label>
                        <input
                            type="number"
                            min="60"
                            step="60"
                            value={projectSettings.refreshInterval ?? 3600}
                            onChange={e => onUpdateProjectSettings({ refreshInterval: parseInt(e.target.value) || 3600 })}
                            className={commonInputClasses}
                            disabled={isRunning}
                        />
                        <p className="text-xs text-gray-500 mt-1">How often the service re-runs this project.</p>
                    </div>
                )}
            </CollapsibleSection>

            <CollapsibleSection title="Actions">
                <div className="flex gap-2">
                    <button onClick={onExport} className={`${commonButtonClasses} bg-indigo-600 hover:bg-indigo-700 flex items-center justify-center gap-2`}>
                        <ArrowDownTrayIcon /> Export JSON
                    </button>
                    <button onClick={onImport} className={`${commonButtonClasses} bg-slate-600 hover:bg-slate-700 flex items-center justify-center gap-2`}>
                        <ArrowUpTrayIcon /> Import JSON
                    </button>
                </div>
                <button
                    onClick={onSave}
                    className="w-full mt-3 p-3 rounded-md font-bold text-white bg-green-600 hover:bg-green-700 transition-colors flex items-center justify-center gap-2 shadow-md"
                >
                    <CloudIcon /> Save Project
                </button>
            </CollapsibleSection>
        </div>
    );

    return (
        <aside className={`fixed top-0 right-0 h-full w-80 bg-white border-l border-gray-200 shadow-xl z-40 flex flex-col transform transition-transform duration-300 ease-in-out ${isOpen ? 'translate-x-0' : 'translate-x-full'}`}>
            <div className="flex justify-between items-center p-6 border-b border-gray-100 flex-shrink-0">
                <h2 className="text-2xl font-bold text-gray-800">{node ? 'Node Settings' : 'Project Config'}</h2>
                <button onClick={onClose} className="p-1 text-gray-500 hover:text-gray-800">
                    <XMarkIcon />
                </button>
            </div>

            <div className="flex-1 overflow-y-auto p-6 space-y-6">
                {node ? (
                    <div className={isRunning ? 'pointer-events-none opacity-60' : ''}>
                        {isRunning && (
                            <div className="bg-amber-50 border border-amber-200 text-amber-800 text-xs font-semibold rounded-lg p-3 mb-4 select-none pointer-events-auto">
                                ⚠️ Service is running. Stop the service in the project config to edit this node.
                            </div>
                        )}
                        {renderNodeSettings()}
                        {node.deletable !== false && (
                            <div className="mt-8 pt-6 border-t">
                                <button
                                    onClick={() => onDeleteNode(node.id)}
                                    disabled={isRunning}
                                    className="w-full p-2 bg-red-100 text-red-700 rounded-md font-semibold hover:bg-red-200 disabled:opacity-50 disabled:cursor-not-allowed transition-colors flex items-center justify-center gap-2"
                                >
                                    <TrashIcon /> Delete Node
                                </button>
                            </div>
                        )}
                    </div>
                ) : renderProjectSettings()}
            </div>

            {!node && (
                <div className="p-6 border-t border-gray-100 bg-slate-50 flex-shrink-0">
                    <button
                        onClick={onSave}
                        className="w-full p-3 rounded-md font-bold text-white bg-green-600 hover:bg-green-700 transition-colors flex items-center justify-center gap-2 shadow-md"
                    >
                        <CloudIcon /> Save Project
                    </button>
                </div>
            )}
        </aside>
    );
};

export default SettingsPanel;
