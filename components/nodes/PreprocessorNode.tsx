import React, { memo } from 'react';
import BaseNode from './BaseNode';
import type { CustomNodeProps, PreprocessorNodeData } from '../../types';
import { FunnelIcon } from '../icons';

const INPUT_TYPE_LABELS: Record<string, string> = {
  html: 'HTML Page',
  csv: 'CSV File',
  json: 'JSON Data',
  xml: 'XML Feed',
  text: 'Plain Text',
};

const PreprocessorNode: React.FC<CustomNodeProps<PreprocessorNodeData>> = ({ data, selected }) => {
  const itemSelector = data.itemSelector || '—';
  const patternCount = data.urlPatterns?.filter(p => p.enabled).length || 0;
  const ruleCount = data.extractRules?.length || 0;

  return (
    <BaseNode
      title="Data Preprocessor"
      icon={<FunnelIcon />}
      selected={selected}
      bgColorClass="bg-purple-50"
    >
      <div className="space-y-2">
        <div className="p-2 bg-white/70 rounded-md">
          <div className="flex items-center justify-between">
            <span className="text-xs font-semibold text-purple-700 uppercase tracking-wide">
              Input Type
            </span>
            <span className="text-sm font-bold text-purple-600">
              {INPUT_TYPE_LABELS[data.inputType] || data.inputType}
            </span>
          </div>
        </div>

        {data.pluginId ? (
          <div className="p-2 bg-blue-50 rounded-md border border-blue-200">
            <p className="text-xs text-blue-700">
              Plugin: <span className="font-semibold">{data.pluginId}</span>
            </p>
          </div>
        ) : null}

        {data.inputType === 'html' && (
          <div className="p-2 bg-white/70 rounded-md">
            <p className="text-xs text-gray-500">CSS Selector</p>
            <p className="text-sm font-mono text-gray-700 truncate" title={itemSelector}>
              {itemSelector}
            </p>
          </div>
        )}

        <div className="flex gap-2">
          {data.inputType === 'html' && (
            <div className="flex-1 p-2 bg-white/70 rounded-md text-center">
              <p className="text-lg font-bold text-purple-600">{patternCount}</p>
              <p className="text-xs text-gray-500">URL Patterns</p>
            </div>
          )}
          <div className="flex-1 p-2 bg-white/70 rounded-md text-center">
            <p className="text-lg font-bold text-purple-600">{ruleCount}</p>
            <p className="text-xs text-gray-500">Extract Rules</p>
          </div>
        </div>

        {(data.csvDelimiter || data.jsonItemPath) && (
          <div className="p-2 bg-white/70 rounded-md">
            {data.csvDelimiter && (
              <p className="text-xs text-gray-500">
                Delimiter: <span className="font-mono font-semibold">'{data.csvDelimiter}'</span>
                {data.csvHasHeader ? ' • Header: Yes' : ' • Header: No'}
              </p>
            )}
            {data.jsonItemPath && (
              <p className="text-xs text-gray-500">
                JSON Path: <span className="font-mono font-semibold">{data.jsonItemPath}</span>
              </p>
            )}
          </div>
        )}

        <p className="text-xs text-gray-400 italic text-center">
          Extracts items from raw data before repository
        </p>
      </div>
    </BaseNode>
  );
};

export default memo(PreprocessorNode);
