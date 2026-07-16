import React, { memo } from 'react';
import { Handle, Position } from 'reactflow';
import type { CustomNodeProps, FetchDataNodeData } from '../../types';
import { CloudArrowDownIcon } from '../icons';

const SOURCE_TYPE_LABELS: Record<string, string> = {
    url: 'HTTP/HTTPS Fetch',
    api: 'API Request',
    csv: 'Read CSV File',
    json: 'Read JSON File',
    xml: 'Read XML Feed',
    mysql: 'MySQL Query',
    plugin: 'Plugin Fetch',
};

const FetchDataNode: React.FC<CustomNodeProps<FetchDataNodeData>> = ({ data, selected }) => {
    const selectionClass = selected ? 'ring-2 ring-blue-500 shadow-2xl' : 'border-slate-300 shadow-lg';
    const strategy = SOURCE_TYPE_LABELS[data.sourceType || ''] || 'Fetch Data';

    return (
        <div className={`bg-sky-50 rounded-lg border w-80 transition-all duration-200 ${selectionClass}`}>
            {/* Header */}
            <div className="flex items-center p-3 border-b border-sky-200 bg-white/70 backdrop-blur-sm rounded-t-lg">
                <div className="text-sky-500 mr-3">
                    <CloudArrowDownIcon />
                </div>
                <h2 className="text-lg font-bold text-gray-700 w-full">Fetch / Get Data</h2>
            </div>

            {/* Body */}
            <div className="p-4 space-y-3">
                {/* Strategy chip */}
                <div className="flex items-center gap-2">
                    <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-semibold bg-sky-100 text-sky-700 border border-sky-200">
                        {strategy}
                    </span>
                </div>

                {/* Description */}
                <div className="p-2 bg-white/60 rounded-md">
                    <p className="text-sm text-gray-600">
                        Retrieves raw data from the upstream data source and passes it downstream for storage.
                    </p>
                </div>

                {/* Status badge */}
                <div className="flex items-center gap-2 p-2 bg-sky-100/60 rounded-md border border-sky-200">
                    <span className="w-2 h-2 rounded-full bg-sky-400 animate-pulse flex-shrink-0" />
                    <p className="text-xs text-sky-700 font-medium">
                        Auto-attached · Read-only
                    </p>
                </div>
            </div>

            <Handle type="target" position={Position.Top} className="w-3 h-3 !bg-sky-400" />
            <Handle type="source" position={Position.Bottom} className="w-3 h-3 !bg-sky-400" />
        </div>
    );
};

export default memo(FetchDataNode);
