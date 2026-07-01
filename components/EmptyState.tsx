import React from 'react';
import { FolderIcon, PlusIcon } from './icons';

interface EmptyStateProps {
    title?: string;
    description?: string;
    actionLabel?: string;
    onAction?: () => void;
}

export const EmptyState: React.FC<EmptyStateProps> = ({
    title = 'No projects yet',
    description = 'Create your first web crawler project to get started.',
    actionLabel = 'Create New Project',
    onAction,
}) => {
    return (
        <div className="text-center py-20">
            <div className="inline-flex items-center justify-center w-16 h-16 bg-gray-200 rounded-full mb-4">
                <FolderIcon />
            </div>
            <h2 className="text-xl font-semibold text-gray-700 mb-2">{title}</h2>
            <p className="text-gray-500 mb-6">{description}</p>
            {onAction && (
                <button
                    onClick={onAction}
                    className="inline-flex items-center gap-2 px-5 py-3 text-sm font-semibold text-white bg-blue-600 rounded-lg hover:bg-blue-700 transition-colors shadow-sm"
                >
                    <PlusIcon /> {actionLabel}
                </button>
            )}
        </div>
    );
};
