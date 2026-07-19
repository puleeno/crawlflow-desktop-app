import React, { useState, useEffect, useCallback } from 'react';
import { GlobeAltIcon, ArrowUpTrayIcon, PlusIcon, Cog6ToothIcon, XMarkIcon } from './icons';
import { CreateProjectForm } from './CreateProjectForm';
import { ProjectCard } from './ProjectCard';
import { EmptyState } from './EmptyState';
import { RawItemsBrowser } from './RawItemsBrowser';
import LiveLogs from './LiveLogs';
import { listProjects, createProject, createProjectFromPreset, deleteProject } from '../lib/db';
import type { Preset, ServiceInfo } from '../types';

interface ProjectRecord {
    id: string;
    name: string;
    description: string;
    status: string;
    db_path: string;
    created_at: string;
    updated_at: string;
}

interface ProjectManagerProps {
    onOpenProject: (projectId: string) => void;
    onImportProject: () => void;
    onOpenSettings?: () => void;
}

export const ProjectManager: React.FC<ProjectManagerProps> = ({ onOpenProject, onImportProject, onOpenSettings }) => {
    const [projects, setProjects] = useState<ProjectRecord[]>([]);
    const [serviceInfos, setServiceInfos] = useState<Record<string, ServiceInfo>>({});
    const [loading, setLoading] = useState(true);
    const [showCreate, setShowCreate] = useState(false);
    const [creating, setCreating] = useState(false);
    const [browseRawProjectId, setBrowseRawProjectId] = useState<string | null>(null);
    const [viewLogsProjectId, setViewLogsProjectId] = useState<string | null>(null);

    const loadProjects = useCallback(async () => {
        setLoading(true);
        try {
            const result = await listProjects();
            setProjects(result as ProjectRecord[]);
        } catch (e) {
            console.error('Failed to load projects:', e);
        }
        setLoading(false);
    }, []);

    useEffect(() => {
        loadProjects();
    }, [loadProjects]);

    // Realtime service status + progress for every project.
    // The GUI process emits `service-status-update` (payload {project_id, info})
    // on every SQLite read; we merge it into the map. A slow poll remains as a
    // fallback to catch any missed event.
    useEffect(() => {
        let cancelled = false;

        const fetchServices = async () => {
            try {
                const { invoke } = await import('@tauri-apps/api/core');
                const infos = await invoke<ServiceInfo[]>('list_project_services_cmd');
                if (cancelled) return;
                const map: Record<string, ServiceInfo> = {};
                for (const info of infos) {
                    map[info.project_id] = info;
                }
                setServiceInfos(map);
            } catch (_) {
                // Not in Tauri env or command unavailable
            }
        };

        const setupEvent = async () => {
            try {
                const { listen } = await import('@tauri-apps/api/event');
                await listen<any>('service-status-update', (event) => {
                    const payload = event.payload;
                    if (payload && payload.project_id && payload.info) {
                        setServiceInfos((prev) => ({
                            ...prev,
                            [payload.project_id]: payload.info as ServiceInfo,
                        }));
                    }
                });
            } catch (_) { /* not in tauri */ }
        };

        fetchServices();
        setupEvent();
        const timer = setInterval(fetchServices, 15000); // fallback only
        return () => {
            cancelled = true;
            clearInterval(timer);
        };
    }, []);

    const handleCreate = async (name: string, description: string) => {
        setCreating(true);
        try {
            const { id } = await createProject(name, description);
            setShowCreate(false);
            onOpenProject(id);
        } catch (e) {
            console.error('Failed to create project:', e);
        }
        setCreating(false);
    };

    const handleApplyPreset = async (preset: Preset) => {
        setCreating(true);
        try {
            const presetName = preset.project_settings.name || preset.name;
            const presetDesc = preset.description;
            const { id } = await createProjectFromPreset(
                presetName,
                presetDesc,
                preset.project_settings,
                preset.nodes,
                preset.edges
            );
            setShowCreate(false);
            onOpenProject(id);
        } catch (e) {
            console.error('Failed to create project from preset:', e);
        }
        setCreating(false);
    };

    const isTauriEnv = () => {
        try {
            return typeof window !== 'undefined' && (
                !!(window as any).__TAURI_INTERNALS__ ||
                !!(window as any).__TAURI__
            );
        } catch {
            return false;
        }
    };

    const handleDelete = async (id: string, name: string) => {
        let confirmed = false;
        if (isTauriEnv()) {
            try {
                const { ask } = await import('@tauri-apps/plugin-dialog');
                confirmed = await ask(`Delete "${name}"?`, { title: 'CrawlFlow', kind: 'warning' });
            } catch { }
        }
        if (!confirmed) {
            confirmed = window.confirm(`Delete "${name}"?`);
        }
        if (confirmed) {
            await deleteProject(id);
            await loadProjects();
        }
    };

    return (
        <div className="min-h-screen bg-gradient-to-br from-slate-50 to-slate-100">
            <div className="max-w-5xl mx-auto px-4 py-8">
                {/* Header */}
                <div className="flex items-center justify-between mb-8">
                    <div className="flex items-center gap-3">
                        <div className="p-2.5 bg-blue-600 rounded-xl shadow-lg text-white">
                            <GlobeAltIcon />
                        </div>
                        <div>
                            <h1 className="text-2xl font-bold text-gray-900">CrawlFlow</h1>
                            <p className="text-sm text-gray-500">Visual Web Crawler Configurator</p>
                        </div>
                    </div>
                    <div className="flex items-center gap-3">
                        {onOpenSettings && (
                            <button
                                onClick={onOpenSettings}
                                className="flex items-center gap-2 px-4 py-2.5 text-sm font-semibold text-gray-600 bg-white border border-gray-200 rounded-lg hover:bg-indigo-50 hover:border-indigo-200 hover:text-indigo-700 transition-colors shadow-sm"
                            >
                                <Cog6ToothIcon /> Settings
                            </button>
                        )}
                        <button
                            onClick={onImportProject}
                            className="flex items-center gap-2 px-4 py-2.5 text-sm font-semibold text-gray-700 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 transition-colors shadow-sm"
                        >
                            <ArrowUpTrayIcon /> Import
                        </button>
                        <button
                            onClick={() => setShowCreate(true)}
                            className="flex items-center gap-2 px-4 py-2.5 text-sm font-semibold text-white bg-blue-600 rounded-lg hover:bg-blue-700 transition-colors shadow-sm"
                        >
                            <PlusIcon /> New Project
                        </button>
                    </div>
                </div>

                {/* Create Form / Preset Selector */}
                {showCreate && (
                    <CreateProjectForm
                        onSubmit={handleCreate}
                        onApplyPreset={handleApplyPreset}
                        onCancel={() => setShowCreate(false)}
                        loading={creating}
                    />
                )}

                {/* Project List */}
                {loading ? (
                    <div className="flex justify-center py-20">
                        <div className="animate-spin w-8 h-8 border-4 border-blue-600 border-t-transparent rounded-full" />
                    </div>
                ) : projects.length === 0 ? (
                    <EmptyState onAction={() => setShowCreate(true)} />
                ) : (
                    <div className="grid gap-4">
                        {projects.map((project) => (
                            <ProjectCard
                                key={project.id}
                                project={project}
                                serviceInfo={serviceInfos[project.id]}
                                onOpen={onOpenProject}
                                onDelete={handleDelete}
                                onBrowseRawItems={setBrowseRawProjectId}
                                onViewLogs={setViewLogsProjectId}
                            />
                        ))}
                    </div>
                )}

                {/* Browse Raw Items Modal */}
                {browseRawProjectId && (
                    <RawItemsBrowser projectId={browseRawProjectId} onClose={() => setBrowseRawProjectId(null)} />
                )}

                {/* View Logs Modal */}
                {viewLogsProjectId && (
                    <LiveLogs projectId={viewLogsProjectId} onClose={() => setViewLogsProjectId(null)} />
                )}
            </div>
        </div>
    );
};