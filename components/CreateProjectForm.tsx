import React, { useState, useEffect } from 'react';
import { listPresets } from '../lib/presets';
import PresetSelector from './PresetSelector';
import type { Preset } from '../types';

interface CreateProjectFormProps {
    onSubmit: (name: string, description: string) => void;
    onApplyPreset: (preset: Preset) => void;
    onCancel: () => void;
    loading?: boolean;
}

export const CreateProjectForm: React.FC<CreateProjectFormProps> = ({ onSubmit, onApplyPreset, onCancel, loading }) => {
    const [mode, setMode] = useState<'presets' | 'manual'>('presets');
    const [name, setName] = useState('');
    const [desc, setDesc] = useState('');
    const [presets, setPresets] = useState<Preset[]>([]);
    const [presetsLoading, setPresetsLoading] = useState(true);

    useEffect(() => {
        listPresets()
            .then(setPresets)
            .catch(() => setPresets([]))
            .finally(() => setPresetsLoading(false));
    }, []);

    const handleSubmit = () => {
        if (name.trim()) {
            onSubmit(name.trim(), desc.trim());
        }
    };

    if (mode === 'presets') {
        return (
            <PresetSelector
                presets={presets}
                loading={presetsLoading}
                onSelectPreset={onApplyPreset}
                onManualCreate={() => setMode('manual')}
                onCancel={onCancel}
            />
        );
    }

    return (
        <div className="mb-6 p-5 bg-white rounded-xl border border-blue-200 shadow-md">
            <h2 className="text-lg font-semibold text-gray-800 mb-4">Create New Project</h2>
            <div className="space-y-3">
                <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">Project Name</label>
                    <input
                        type="text"
                        value={name}
                        onChange={(e) => setName(e.target.value)}
                        placeholder="My Crawler Project"
                        className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500 outline-none"
                        autoFocus
                        onKeyDown={(e) => e.key === 'Enter' && handleSubmit()}
                    />
                </div>
                <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">Description (optional)</label>
                    <textarea
                        value={desc}
                        onChange={(e) => setDesc(e.target.value)}
                        placeholder="What does this crawler do?"
                        rows={2}
                        className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500 outline-none resize-none"
                    />
                </div>
                <div className="flex items-center gap-2 pt-1">
                    <button
                        onClick={() => setMode('presets')}
                        className="px-3 py-1.5 text-xs font-medium text-gray-500 bg-gray-100 rounded-lg hover:bg-gray-200 transition-colors"
                    >
                        ← Quick Setup
                    </button>
                    <div className="flex-1" />
                    <button
                        onClick={onCancel}
                        className="px-4 py-2 text-sm font-semibold text-gray-600 bg-gray-100 rounded-lg hover:bg-gray-200 transition-colors"
                    >
                        Cancel
                    </button>
                    <button
                        onClick={handleSubmit}
                        disabled={!name.trim() || loading}
                        className="px-4 py-2 text-sm font-semibold text-white bg-blue-600 rounded-lg hover:bg-blue-700 disabled:bg-blue-300 transition-colors"
                    >
                        {loading ? 'Creating...' : 'Create & Open'}
                    </button>
                </div>
            </div>
        </div>
    );
};