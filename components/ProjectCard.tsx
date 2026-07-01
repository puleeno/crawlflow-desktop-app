import React from 'react';
import { FolderIcon, TrashIcon } from './icons';

interface ProjectRecord {
    id: string;
    name: string;
    description: string;
    status: string;
    updated_at: string;
}

interface ProjectCardProps {
    project: ProjectRecord;
    onOpen: (id: string) => void;
    onDelete: (id: string, name: string) => void;
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

export const ProjectCard: React.FC<ProjectCardProps> = ({ project, onOpen, onDelete }) => {
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
                </div>
                {project.description && (
                    <p className="text-sm text-gray-500 truncate">{project.description}</p>
                )}
                <p className="text-xs text-gray-400 mt-1">
                    Updated {formatDate(project.updated_at)}
                </p>
            </div>
            <button
                onClick={(e) => { e.stopPropagation(); onDelete(project.id, project.name); }}
                className="flex-shrink-0 p-2 text-gray-400 hover:text-red-500 hover:bg-red-50 rounded-lg opacity-0 group-hover:opacity-100 transition-all"
                title="Delete project"
            >
                <TrashIcon />
            </button>
        </div>
    );
};
