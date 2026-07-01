import React, { useState, useEffect, useCallback } from 'react';
import { GlobeAltIcon, ArrowUpTrayIcon, PlusIcon } from './icons';
import { CreateProjectForm } from './CreateProjectForm';
import { ProjectCard } from './ProjectCard';
import { EmptyState } from './EmptyState';
import { listProjects, createProject, deleteProject } from '../lib/db';

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
}

export const ProjectManager: React.FC<ProjectManagerProps> = ({ onOpenProject, onImportProject }) => {
    const [projects, setProjects] = useState<ProjectRecord[]>([]);
    const [loading, setLoading] = useState(true);
    const [showCreate, setShowCreate] = useState(false);
    const [creating, setCreating] = useState(false);

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

    const handleDelete = async (id: string, name: string) => {
        try {
            const { ask } = await import('@tauri-apps/plugin-dialog');
            const confirmed = await ask(`Delete "${name}"?`, { title: 'CrawlFlow', kind: 'warning' });
            if (confirmed) {
                await deleteProject(id);
                await loadProjects();
            }
        } catch {
            if (window.confirm(`Delete "${name}"?`)) {
                await deleteProject(id);
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

                {/* Create Form */}
                {showCreate && (
                    <CreateProjectForm
                        onSubmit={handleCreate}
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
                                onOpen={onOpenProject}
                                onDelete={handleDelete}
                            />
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
};
