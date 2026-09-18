import React from 'react';
import { FolderIcon, TrashIcon, TableCellsIcon, DocumentTextIcon } from './icons';
import type { ServiceInfo } from '../types';

interface ProjectRecord {
    id: string;
    name: string;
    description: string;
    status: string;
    updated_at: string;
}

interface ProjectCardProps {
    project: ProjectRecord;
    serviceInfo?: ServiceInfo;
    onOpen: (id: string) => void;
    onDelete: (id: string, name: string) => void;
    onBrowseRawItems: (id: string) => void;
    onViewLogs: (id: string) => void;
}

const getStatusColor = (status: string) => {
    switch (status.toLowerCase()) {
        case 'enabled':
        case 'running': return 'bg-green-100 text-green-800';
        case 'disabled':
        case 'paused': return 'bg-yellow-100 text-yellow-800';
        case 'completed': return 'bg-blue-100 text-blue-800';
        default: return 'bg-gray-100 text-gray-600';
    }
};

const getRuntimeStatus = (serviceInfo?: ServiceInfo): { label: string; color: string; dot: string } => {
    if (!serviceInfo) return { label: 'Idle', color: 'bg-gray-100 text-gray-600', dot: 'bg-gray-400' };
    const s = serviceInfo.status;
    if (s === 'running') return { label: 'Running', color: 'bg-green-100 text-green-700', dot: 'bg-green-500 animate-pulse' };
    if (s === 'idle') return { label: 'Idle', color: 'bg-slate-100 text-slate-600', dot: 'bg-slate-400' };
    if (s === 'paused') return { label: 'Paused', color: 'bg-amber-100 text-amber-700', dot: 'bg-amber-500' };
    if (s === 'completed') return { label: 'Completed', color: 'bg-blue-100 text-blue-700', dot: 'bg-blue-500' };
    if (s === 'disabled') return { label: 'Disabled', color: 'bg-yellow-100 text-yellow-700', dot: 'bg-yellow-500' };
    if (s.startsWith('error')) return { label: 'Error', color: 'bg-red-100 text-red-700', dot: 'bg-red-500' };
    return { label: 'Stopped', color: 'bg-gray-100 text-gray-600', dot: 'bg-gray-400' };
};

const formatDate = (dateStr: string) => {
    try {
        const d = new Date(dateStr);
        return d.toLocaleDateString('vi-VN', {
            year: 'numeric', month: 'short', day: 'numeric',
            hour: '2-digit', minute: '2-digit',
        });
    } catch {
        return dateStr;
    }
};

export const ProjectCard: React.FC<ProjectCardProps> = ({ project, serviceInfo, onOpen, onDelete, onBrowseRawItems, onViewLogs }) => {
    const runtime = getRuntimeStatus(serviceInfo);
    const progress = serviceInfo?.progress;
    const pct = progress ? Math.max(0, Math.min(100, progress.progress_pct)) : 0;
    const isActive = !!serviceInfo && !!progress && (runtime.label === 'Running' || runtime.label === 'Idle');
    const hasProgress = !!progress && (progress.items_total > 0 || progress.items_processed > 0);

    return (
        <div
            onClick={() => onOpen(project.id)}
            className="group flex items-center gap-4 p-4 bg-white rounded-xl border border-gray-200 shadow-sm hover:shadow-md hover:border-blue-200 transition-all cursor-pointer"
        >
            <div className="flex-shrink-0 w-10 h-10 bg-blue-50 rounded-lg flex items-center justify-center text-blue-600">
                <FolderIcon />
            </div>
            <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-0.5">
                    <h3 className="text-base font-semibold text-gray-900 truncate">
                        {project.name}
                    </h3>
                    <span className={`text-xs font-medium px-2 py-0.5 rounded-full capitalize ${getStatusColor(project.status)}`}>
                        {project.status}
                    </span>
                    <span className={`inline-flex items-center gap-1 text-xs font-medium px-2 py-0.5 rounded-full capitalize ${runtime.color}`}>
                        <span className={`inline-block w-1.5 h-1.5 rounded-full ${runtime.dot}`} />
                        {runtime.label}
                    </span>
                    {serviceInfo && serviceInfo.cycle_count > 0 && (
                        <span className="text-xs text-gray-400">#{serviceInfo.cycle_count}</span>
                    )}
                </div>
                {project.description && (
                    <p className="text-sm text-gray-500 truncate">{project.description}</p>
                )}

                {/* Realtime progress bar */}
                {isActive && (
                    <div className="mt-2">
                        <div className="flex items-center justify-between text-[11px] text-gray-500 mb-1">
                            <span className="truncate max-w-[70%]">{progress!.message || 'Processing…'}</span>
                            <span className="shrink-0 font-mono">{pct.toFixed(0)}%</span>
                        </div>
                        <div className="w-full h-1.5 bg-gray-100 rounded-full overflow-hidden">
                            <div
                                className={`h-full rounded-full transition-all duration-500 ${progress!.items_failed > 0 && pct >= 100 ? 'bg-amber-500' : 'bg-blue-500'}`}
                                style={{ width: `${pct}%` }}
                            />
                        </div>
                        <div className="flex items-center gap-3 mt-1 text-[11px] text-gray-400">
                            <span>Total: {progress!.items_total}</span>
                            <span className="text-green-600">Done: {progress!.items_success}</span>
                            {progress!.items_failed > 0 && <span className="text-red-500">Failed: {progress!.items_failed}</span>}
                            <span>Pending: {progress!.items_pending}</span>
                        </div>
                    </div>
                )}

                <p className="text-xs text-gray-400 mt-1">
                    Updated {formatDate(project.updated_at)}
                </p>
            </div>
            <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all">
                <button
                    onClick={(e) => { e.stopPropagation(); onBrowseRawItems(project.id); }}
                    className="p-2 text-gray-400 hover:text-indigo-600 hover:bg-indigo-50 rounded-lg transition-colors"
                    title="Browse raw items"
                >
                    <TableCellsIcon />
                </button>
                <button
                    onClick={(e) => { e.stopPropagation(); onViewLogs(project.id); }}
                    className="p-2 text-gray-400 hover:text-amber-600 hover:bg-amber-50 rounded-lg transition-colors"
                    title="View logs"
                >
                    <DocumentTextIcon />
                </button>
                <button
                    onClick={(e) => { e.stopPropagation(); onDelete(project.id, project.name); }}
                    className="p-2 text-gray-400 hover:text-red-500 hover:bg-red-50 rounded-lg transition-colors"
                    title="Delete project"
                >
                    <TrashIcon />
                </button>
            </div>
        </div>
    );
};
