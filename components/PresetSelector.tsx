import React from 'react';
import type { Preset } from '../types';
import { XMarkIcon, PlusIcon } from './icons';

interface PresetSelectorProps {
    presets: Preset[];
    loading: boolean;
    onSelectPreset: (preset: Preset) => void;
    onManualCreate: () => void;
    onCancel: () => void;
}

const ICON_MAP: Record<string, string> = {
    GlobeAltIcon: '🌐',
    RssIcon: '📡',
    ShoppingCartIcon: '🛒',
};

const PresetSelector: React.FC<PresetSelectorProps> = ({ presets, loading, onSelectPreset, onManualCreate, onCancel }) => {
    const getIcon = (preset: Preset) => ICON_MAP[preset.icon] || '📦';

    return (
        <div className="mb-6 p-5 bg-white rounded-xl border border-gray-200 shadow-md">
            <div className="flex items-center justify-between mb-4">
                <h2 className="text-lg font-semibold text-gray-800">Quick Setup</h2>
                <button onClick={onCancel} className="p-1 text-gray-400 hover:text-gray-700 rounded-lg hover:bg-gray-100 transition-colors">
                    <XMarkIcon />
                </button>
            </div>

            {loading ? (
                <div className="flex justify-center py-8">
                    <div className="animate-spin w-6 h-6 border-4 border-blue-600 border-t-transparent rounded-full" />
                </div>
            ) : (
                <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3 mb-4">
                    {presets.map((preset) => (
                        <button
                            key={preset.id}
                            onClick={() => onSelectPreset(preset)}
                            className="p-4 bg-gray-50 rounded-xl border border-gray-200 hover:border-blue-400 hover:shadow-md transition-all text-left group"
                        >
                            <div className="flex items-center gap-3 mb-2">
                                <span className="text-2xl">{getIcon(preset)}</span>
                                <div className="flex-1 min-w-0">
                                    <h3 className="text-sm font-semibold text-gray-900 truncate">{preset.name}</h3>
                                    {preset.source === 'plugin' && (
                                        <span className="text-[10px] font-medium text-purple-600 bg-purple-50 px-1.5 py-0.5 rounded-full">
                                            Plugin
                                        </span>
                                    )}
                                </div>
                            </div>
                            <p className="text-xs text-gray-500 line-clamp-2">{preset.description}</p>
                        </button>
                    ))}
                </div>
            )}

            <div className="flex items-center gap-2 pt-3 border-t border-gray-100">
                <div className="flex-1 h-px bg-gray-200" />
                <span className="text-xs text-gray-400 font-medium">OR</span>
                <div className="flex-1 h-px bg-gray-200" />
            </div>

            <button
                onClick={onManualCreate}
                className="mt-3 w-full flex items-center justify-center gap-2 px-4 py-2.5 text-sm font-semibold text-gray-700 bg-gray-100 rounded-lg hover:bg-gray-200 transition-colors"
            >
                <PlusIcon /> Start from scratch
            </button>
        </div>
    );
};

export default PresetSelector;