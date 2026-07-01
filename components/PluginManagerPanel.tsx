import React, { useState, useEffect } from 'react';
import { pluginManager } from '../lib/pluginManager';
import { XMarkIcon, Cog6ToothIcon, PlusIcon } from './icons';
import type { CrawlFlowPlugin, PluginConfigField } from '../types';
import MarketplacePanel from './MarketplacePanel';

interface PluginManagerPanelProps {
    isOpen: boolean;
    onClose: () => void;
}

const capabilityLabels: Record<string, string> = {
    hook: 'Pipeline Hook',
    dataSource: 'Data Source',
    processor: 'Processor',
    parser: 'Parser',
};

const capabilityColors: Record<string, string> = {
    hook: 'bg-purple-100 text-purple-800',
    dataSource: 'bg-blue-100 text-blue-800',
    processor: 'bg-green-100 text-green-800',
    parser: 'bg-orange-100 text-orange-800',
};

const PluginConfigForm: React.FC<{
    fields: PluginConfigField[];
    values: Record<string, any>;
    onChange: (values: Record<string, any>) => void;
}> = ({ fields, values, onChange }) => {
    const set = (key: string, val: any) => onChange({ ...values, [key]: val });

    return (
        <div className="space-y-3 mt-3 p-3 bg-slate-50 rounded-lg">
            <p className="text-xs font-semibold text-gray-500 uppercase tracking-wide">Configuration</p>
            {fields.map(f => (
                <div key={f.key}>
                    <label className="block text-xs font-medium text-gray-700 mb-1">{f.label}</label>
                    {f.type === 'boolean' ? (
                        <label className="flex items-center gap-2 cursor-pointer">
                            <input type="checkbox" checked={!!values[f.key]} onChange={e => set(f.key, e.target.checked)}
                                className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500" />
                            <span className="text-sm text-gray-600">Enabled</span>
                        </label>
                    ) : f.type === 'select' ? (
                        <select value={values[f.key] ?? f.defaultValue ?? ''} onChange={e => set(f.key, e.target.value)}
                            className="w-full p-2 text-sm bg-white border border-gray-300 rounded-md focus:ring-2 focus:ring-blue-500">
                            {f.options?.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
                        </select>
                    ) : f.type === 'textarea' ? (
                        <textarea value={values[f.key] ?? f.defaultValue ?? ''} onChange={e => set(f.key, e.target.value)}
                            placeholder={f.placeholder}
                            rows={3}
                            className="w-full p-2 text-sm bg-white border border-gray-300 rounded-md focus:ring-2 focus:ring-blue-500 font-mono" />
                    ) : (
                        <input type={f.type === 'number' ? 'number' : 'text'}
                            value={values[f.key] ?? f.defaultValue ?? ''}
                            onChange={e => set(f.key, f.type === 'number' ? parseFloat(e.target.value) || 0 : e.target.value)}
                            placeholder={f.placeholder}
                            className="w-full p-2 text-sm bg-white border border-gray-300 rounded-md focus:ring-2 focus:ring-blue-500" />
                    )}
                </div>
            ))}
        </div>
    );
};

const PluginCard: React.FC<{
    plugin: CrawlFlowPlugin;
    onToggle: (id: string, enabled: boolean) => void;
    enabled: boolean;
}> = ({ plugin, onToggle, enabled }) => {
    const [showConfig, setShowConfig] = useState(false);
    const [config, setConfig] = useState<Record<string, any>>(plugin.defaultConfig || {});
    const cfgFields = plugin.configFields || [];

    return (
        <div className={`p-4 bg-white rounded-xl border transition-all ${enabled ? 'border-blue-200 shadow-sm' : 'border-gray-200 opacity-70'}`}>
            <div className="flex items-start justify-between gap-3">
                <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                        <h3 className="text-sm font-semibold text-gray-900">{plugin.name}</h3>
                        <span className="text-xs text-gray-400">v{plugin.version}</span>
                    </div>
                    <p className="text-xs text-gray-500 mb-2">{plugin.description}</p>
                    <div className="flex flex-wrap gap-1.5">
                        {plugin.capabilities.map(cap => (
                            <span key={cap} className={`text-xs font-medium px-2 py-0.5 rounded-full ${capabilityColors[cap] || 'bg-gray-100 text-gray-600'}`}>
                                {capabilityLabels[cap] || cap}
                            </span>
                        ))}
                        {plugin.author && (
                            <span className="text-xs text-gray-400">by {plugin.author}</span>
                        )}
                    </div>
                </div>
                <label className="relative inline-flex items-center cursor-pointer flex-shrink-0">
                    <input type="checkbox" checked={enabled} onChange={e => onToggle(plugin.id, e.target.checked)}
                        className="sr-only peer" />
                    <div className="w-9 h-5 bg-gray-200 peer-focus:outline-none peer-focus:ring-2 peer-focus:ring-blue-300 rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:start-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-blue-600" />
                </label>
            </div>

            {enabled && cfgFields.length > 0 && (
                <div className="mt-2">
                    <button onClick={() => setShowConfig(!showConfig)}
                        className="flex items-center gap-1 text-xs font-medium text-gray-500 hover:text-gray-700 transition-colors">
                        <Cog6ToothIcon /> {showConfig ? 'Hide Config' : 'Configure'}
                    </button>
                    {showConfig && (
                        <PluginConfigForm fields={cfgFields} values={config} onChange={setConfig} />
                    )}
                </div>
            )}
        </div>
    );
};

export const PluginManagerPanel: React.FC<PluginManagerPanelProps> = ({ isOpen, onClose }) => {
    const [plugins, setPlugins] = useState<CrawlFlowPlugin[]>([]);
    const [enabled, setEnabled] = useState<Set<string>>(new Set());
    const [marketplaceOpen, setMarketplaceOpen] = useState(false);

    const refresh = () => {
        setPlugins(pluginManager.getAllPlugins());
        setEnabled(new Set(pluginManager.getEnabledPlugins().map(p => p.id)));
    };

    useEffect(() => {
        if (isOpen) refresh();
    }, [isOpen]);

    const handleToggle = async (id: string, val: boolean) => {
        await pluginManager.setEnabled(id, val);
        refresh();
    };

    if (!isOpen) return null;

    const hookPlugins = plugins.filter(p => p.capabilities.includes('hook'));
    const dsPlugins = plugins.filter(p => p.capabilities.includes('dataSource'));
    const procPlugins = plugins.filter(p => p.capabilities.includes('processor'));
    const parserPlugins = plugins.filter(p => p.capabilities.includes('parser'));

    const renderSection = (title: string, items: CrawlFlowPlugin[]) => {
        if (items.length === 0) return null;
        return (
            <div className="mb-6">
                <h3 className="text-sm font-bold text-gray-700 mb-3 uppercase tracking-wider">{title}</h3>
                <div className="space-y-3">
                    {items.map(p => (
                        <PluginCard key={p.id} plugin={p} onToggle={handleToggle} enabled={enabled.has(p.id)} />
                    ))}
                </div>
            <MarketplacePanel isOpen={marketplaceOpen} onClose={() => setMarketplaceOpen(false)} />
        </div>
    );
};

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
            <div className="bg-white rounded-2xl shadow-2xl w-full max-w-2xl max-h-[85vh] overflow-hidden flex flex-col">
                <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200">
                    <div>
                        <h2 className="text-lg font-bold text-gray-900">Plugin Manager</h2>
                        <p className="text-xs text-gray-500">Enable/disable plugins and configure their behavior</p>
                    </div>
                    <div className="flex items-center gap-2">
                        <button
                            onClick={() => setMarketplaceOpen(true)}
                            className="px-3 py-1.5 text-xs font-medium bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition-colors"
                        >
                            Browse Marketplace
                        </button>
                        <button onClick={onClose} className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors">
                            <XMarkIcon />
                        </button>
                    </div>
                </div>

                <div className="flex-1 overflow-y-auto px-6 py-4">
                    {plugins.length === 0 ? (
                        <div className="text-center py-10 text-gray-500 text-sm">No plugins registered.</div>
                    ) : (
                        <>
                            {renderSection('Pipeline Hooks', hookPlugins)}
                            {renderSection('Data Sources', dsPlugins)}
                            {renderSection('Processors', procPlugins)}
                            {renderSection('Parsers', parserPlugins)}
                        </>
                    )}
                </div>
            </div>
        </div>
    );
};
