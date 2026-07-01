import React, { useState, useEffect, useCallback } from 'react';
import { fetchItems, resolveDownload, type MarketplaceItem } from '../lib/marketplace';
import { XMarkIcon, SearchIcon } from './icons';
import { invoke } from '@tauri-apps/api/core';

interface MarketplacePanelProps {
    isOpen: boolean;
    onClose: () => void;
}

type Tab = 'plugins' | 'templates';

const MarketplacePanel: React.FC<MarketplacePanelProps> = ({ isOpen, onClose }) => {
    const [plugins, setPlugins] = useState<MarketplaceItem[]>([]);
    const [templates, setTemplates] = useState<MarketplaceItem[]>([]);
    const [loading, setLoading] = useState(true);
    const [activeTab, setActiveTab] = useState<Tab>('plugins');
    const [searchQuery, setSearchQuery] = useState('');
    const [installing, setInstalling] = useState<string | null>(null);
    const [installMsg, setInstallMsg] = useState<{ slug: string; ok: boolean; msg: string } | null>(null);

    const load = useCallback(async () => {
        setLoading(true);
        try {
            const [pluginResp, templateResp] = await Promise.all([
                fetchItems({ type: 'plugin' }),
                fetchItems({ type: 'template' }),
            ]);
            setPlugins(pluginResp.data);
            setTemplates(templateResp.data);
        } catch (err) {
            console.error('Failed to fetch marketplace items', err);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        if (isOpen) load();
    }, [isOpen, load]);

    const handleInstall = async (item: MarketplaceItem) => {
        setInstalling(item.slug);
        setInstallMsg(null);
        try {
            const downloadUrl = await resolveDownload(item.slug);
            if (!downloadUrl) {
                setInstallMsg({ slug: item.slug, ok: false, msg: 'No download URL available' });
                return;
            }
            const result = await invoke<string>('install_marketplace_item', {
                slug: item.slug,
                itemType: item.item_type,
                downloadUrl,
            });
            setInstallMsg({ slug: item.slug, ok: true, msg: `Installed at ${result}` });
        } catch (err: any) {
            setInstallMsg({ slug: item.slug, ok: false, msg: String(err) });
        } finally {
            setInstalling(null);
        }
    };

    const items = activeTab === 'plugins' ? plugins : templates;
    const filtered = items.filter(item =>
        item.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        item.description.toLowerCase().includes(searchQuery.toLowerCase())
    );

    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
            <div className="bg-white rounded-2xl shadow-2xl w-full max-w-4xl max-h-[85vh] overflow-hidden flex flex-col">
                <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200">
                    <div>
                        <h2 className="text-lg font-bold text-gray-900">Marketplace</h2>
                        <p className="text-xs text-gray-500">Browse and install plugins & templates from CrawlFlow Marketplace</p>
                    </div>
                    <button onClick={onClose} className="p-1.5 text-gray-400 hover:text-gray-700 hover:bg-gray-100 rounded-lg transition-colors">
                        <XMarkIcon />
                    </button>
                </div>

                <div className="px-6 py-3 border-b border-gray-100 flex items-center gap-4">
                    <div className="flex gap-1 bg-gray-100 rounded-lg p-0.5">
                        {(['plugins', 'templates'] as const).map(t => (
                            <button
                                key={t}
                                onClick={() => { setActiveTab(t); setSearchQuery(''); }}
                                className={`px-4 py-1.5 text-xs font-medium rounded-md transition-colors capitalize ${
                                    activeTab === t
                                        ? 'bg-white text-gray-900 shadow-sm'
                                        : 'text-gray-500 hover:text-gray-700'
                                }`}
                            >
                                {t}
                            </button>
                        ))}
                    </div>
                    <div className="flex-1 relative">
                        <input
                            type="text"
                            placeholder={`Search ${activeTab}...`}
                            value={searchQuery}
                            onChange={e => setSearchQuery(e.target.value)}
                            className="w-full pl-9 pr-3 py-2 text-sm bg-gray-50 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                        />
                        <span className="absolute left-3 top-2.5 text-gray-400"><SearchIcon /></span>
                    </div>
                </div>

                <div className="flex-1 overflow-y-auto px-6 py-4">
                    {loading ? (
                        <div className="text-center py-16 text-gray-400 text-sm">Loading...</div>
                    ) : filtered.length === 0 ? (
                        <div className="text-center py-16 text-gray-400 text-sm">No {activeTab} found.</div>
                    ) : (
                        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                            {filtered.map(item => (
                                <div key={item.id} className="p-4 bg-white rounded-xl border border-gray-200 hover:shadow-md transition-shadow flex flex-col">
                                    <div className="flex items-start gap-3 mb-3">
                                        <div
                                            className="w-10 h-10 rounded-lg flex items-center justify-center flex-shrink-0 text-white text-lg font-bold"
                                            style={{ backgroundColor: item.icon_color || '#6366f1' }}
                                        >
                                            {item.name.charAt(0)}
                                        </div>
                                        <div className="flex-1 min-w-0">
                                            <h3 className="text-sm font-semibold text-gray-900 truncate">{item.name}</h3>
                                            <div className="flex items-center gap-2 mt-0.5">
                                                <span className="text-xs font-medium px-1.5 py-0.5 rounded-full bg-blue-50 text-blue-700">
                                                    {item.item_type === 'template' ? 'Template' : 'Plugin'}
                                                </span>
                                                <span className="text-xs text-gray-400">v{item.latest_version || '1.0.0'}</span>
                                            </div>
                                        </div>
                                    </div>
                                    <p className="text-xs text-gray-500 mb-3 line-clamp-2 flex-1">{item.description}</p>
                                    <div className="flex items-center justify-between pt-3 border-t border-gray-100">
                                        <div className="flex items-center gap-2">
                                            {item.price && item.price > 0 ? (
                                                <span className="text-sm font-bold text-gray-900">
                                                    {item.currency === 'VND' ? '₫' : '$'}{item.price}
                                                </span>
                                            ) : (
                                                <span className="text-xs font-medium text-green-600 bg-green-50 px-2 py-0.5 rounded-full">Free</span>
                                            )}
                                            <span className="text-xs text-gray-400">{item.install_count || 0} installs</span>
                                        </div>
                                        <button
                                            onClick={() => handleInstall(item)}
                                            disabled={installing === item.slug}
                                            className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-colors ${
                                                installing === item.slug
                                                    ? 'bg-gray-100 text-gray-400 cursor-wait'
                                                    : 'bg-blue-600 text-white hover:bg-blue-700'
                                            }`}
                                        >
                                            {installing === item.slug ? 'Installing...' : 'Install'}
                                        </button>
                                    </div>
                                    {installMsg && installMsg.slug === item.slug && (
                                        <div className={`mt-2 text-xs p-2 rounded-md ${installMsg.ok ? 'bg-green-50 text-green-700' : 'bg-red-50 text-red-700'}`}>
                                            {installMsg.msg}
                                        </div>
                                    )}
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
};

export default MarketplacePanel;
