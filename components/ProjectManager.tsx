import React, { useState, useEffect, useCallback } from 'react';
import { GlobeAltIcon, ArrowUpTrayIcon, PlusIcon, Cog6ToothIcon, XMarkIcon } from './icons';
import { CreateProjectForm } from './CreateProjectForm';
import { ProjectCard } from './ProjectCard';
import { EmptyState } from './EmptyState';
import { RawItemsBrowser } from './RawItemsBrowser';
import LiveLogs from './LiveLogs';
import { listProjects, createProject, createProjectFromPreset, deleteProject } from '../lib/db';
import type { Preset, ServiceInfo } from '../types';
import { ProjectWsClient } from '@/wsClient';

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
    /** Called when a preset is selected — loads it into the editor as an unsaved draft. */
    onApplyPreset?: (preset: Preset) => void;
}

export const ProjectManager: React.FC<ProjectManagerProps> = ({ onOpenProject, onImportProject, onOpenSettings, onApplyPreset }) => {
    const [projects, setProjects] = useState<ProjectRecord[]>([]);
    const [serviceInfos, setServiceInfos] = useState<Record<string, ServiceInfo>>({});
    const [loading, setLoading] = useState(true);
    const [showCreate, setShowCreate] = useState(false);
    const [creating, setCreating] = useState(false);
    const [browseRawProjectId, setBrowseRawProjectId] = useState<string | null>(null);
    const [viewLogsProjectId, setViewLogsProjectId] = useState<string | null>(null);

    // Tracks which project IDs have received at least one WS progress frame.
    // Used to prevent SQLite/Tauri snapshots from overwriting live WS progress.
    const wsProgressReceived = React.useRef<Set<string>>(new Set());

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
                const { invoke } = await import('../lib/platform');
                const infos = await invoke<ServiceInfo[]>('list_project_services_cmd');
                if (cancelled) return;
                // Merge: never overwrite WS-supplied progress with a SQLite snapshot.
                setServiceInfos((prev) => {
                    const next: Record<string, ServiceInfo> = {};
                    for (const info of infos) {
                        const existing = prev[info.project_id];
                        next[info.project_id] = {
                            ...info,
                            progress: wsProgressReceived.current.has(info.project_id) && existing?.progress
                                ? existing.progress
                                : info.progress,
                        };
                    }
                    return next;
                });
            } catch (_) {
                // Not in Tauri env or command unavailable
            }
        };

        const setupEvent = async () => {
            try {
                const { listen } = await import('../lib/platform');
                await listen<any>('service-status-update', (event) => {
                    const payload = event.payload;
                    if (payload && payload.project_id && payload.info) {
                        setServiceInfos((prev) => {
                            const existing = prev[payload.project_id];
                            const newInfo = payload.info as ServiceInfo;
                            // Never override WS-delivered progress with a Tauri/SQLite snapshot.
                            const preserveProgress = wsProgressReceived.current.has(payload.project_id) && existing?.progress;
                            return {
                                ...prev,
                                [payload.project_id]: {
                                    ...newInfo,
                                    progress: preserveProgress ? existing!.progress : newInfo.progress,
                                },
                            };
                        });
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

    // Realtime progress over WebSocket for every running project. The
    // control-plane status still arrives via the Tauri event above; here we
    // subscribe to each project's WS server (port from ServiceInfo.ws_port)
    // to push progress frames live into the same map. We only (re)connect when
    // the set of (project_id, port) endpoints actually changes, so frequent
    // progress updates don't churn sockets.
    const wsSignature = Object.values(serviceInfos as Record<string, ServiceInfo>)
        .map((i) => `${i.project_id}:${i.ws_port || 0}`)
        .filter((s) => !s.endsWith(':0'))
        .sort()
        .join('|');
    useEffect(() => {
        const clients: ProjectWsClient[] = [];
        for (const info of Object.values(serviceInfos as Record<string, ServiceInfo>)) {
            const port = info.ws_port || 0;
            if (port === 0) continue;
            const client = new ProjectWsClient(info.project_id, {
                onProgress: (payload) => {
                    if (!payload) return;
                    // Mark that this project has live WS progress — SQLite snapshots
                    // must not overwrite it from here on.
                    wsProgressReceived.current.add(info.project_id);
                    setServiceInfos((prev) => {
                        const existing = prev[info.project_id];
                        if (!existing) return prev;
                        // Avoid glitching: Ignore empty Ticker progress if the plugin is manually driving it
                        if (
                            payload.items_total === 0 &&
                            payload.phase === 'running' &&
                            existing.progress?.phase === 'fetching'
                        ) {
                            // Only update the message from the ticker (so live logs still show)
                            return {
                                ...prev,
                                [info.project_id]: {
                                    ...existing,
                                    progress: {
                                        ...existing.progress,
                                        message: payload.message || existing.progress.message,
                                    },
                                    ws_port: port,
                                },
                            };
                        }

                        return {
                            ...prev,
                            [info.project_id]: {
                                ...existing,
                                progress: payload,
                                ws_port: port,
                            },
                        };
                    });
                },
                onStatus: (payload) => {
                    // When the service stops, clear the WS-received flag so the
                    // next run can bootstrap progress from the SQLite snapshot again.
                    if (payload?.status && payload.status !== 'running' && payload.status !== 'idle') {
                        wsProgressReceived.current.delete(info.project_id);
                    }
                },
            });
            client.connect(port);
            clients.push(client);
        }
        return () => {
            for (const c of clients) c.disconnect();
        };
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [wsSignature]);

    const handleCreate = async (name: string, description: string) => {
        setCreating(true);
        try {
            const { id } = await createProject(name, description);
            setShowCreate(false);
            onOpenProject(id);
        } catch (e) {
            console.error('Failed to create project:', e);
            alert(e instanceof Error ? e.message : 'Failed to create project');
        }
        setCreating(false);
    };

    const handleApplyPreset = async (preset: Preset) => {
        // Prefer loading the preset as an unsaved draft into the editor; fall
        // back to creating a project immediately when no draft handler exists.
        if (onApplyPreset) {
            onApplyPreset(preset);
            setShowCreate(false);
            return;
        }
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
            alert(e instanceof Error ? e.message : 'Failed to create project from preset');
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
                const { askDialog } = await import('../lib/platform');
                confirmed = await askDialog(`Delete "${name}"?`, { title: 'CrawlFlow', kind: 'warning' });
            } catch { }
        }
        if (!confirmed) {
            confirmed = window.confirm(`Delete "${name}"?`);
        }
        if (confirmed) {
            // Optimistically drop the row so the list refreshes immediately,
            // then reconcile from the DB. Deleting never leaves the UI stale.
            setProjects((prev) => prev.filter((p) => p.id !== id));
            try {
                await deleteProject(id);
            } catch (e) {
                console.error(`Failed to delete project ${id}:`, e);
                alert(e instanceof Error ? e.message : `Failed to delete "${name}"`);
            } finally {
                await loadProjects();
            }
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