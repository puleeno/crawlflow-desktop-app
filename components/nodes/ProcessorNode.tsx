import React, { memo } from 'react';
import BaseNode from './BaseNode';
import type { CustomNodeProps, ProcessorNodeData } from '../../types';
import { Cog6ToothIcon } from '../icons';
import { PROCESSORS } from '../../presets';

const ProcessorNode: React.FC<CustomNodeProps<ProcessorNodeData>> = ({ data, selected }) => {
  const processor = PROCESSORS.find(p => p.id === data.processorType);
  // Fall back to processorConfig (used by preset nodes) if settings is not present
  const settings: any = (data as any).settings ?? (data as any).processorConfig ?? {};
  const processorName = processor ? processor.name : (data as any).label || 'Not Selected';

  const renderSummary = () => {
    switch (data.processorType) {
      case 'save-to-database':
        return `Table: ${settings?.tableName || 'N/A'} (${settings?.conflictStrategy || settings?.strategy || 'insert'})`;

      case 'send-to-api': {
        const url = (settings?.endpointUrl || '').replace(/^https?:\/\//, '');
        const displayUrl = url.length > 25 ? `${url.substring(0, 25)}...` : url;
        return `${settings?.method || 'POST'} to ${displayUrl || 'N/A'}`;
      }

      case 'generate-csv-file':
        return `Delimiter: ${settings?.delimiter || ','} · Header: ${settings?.includeHeader !== false ? 'Yes' : 'No'}`;

      case 'generate-excel-file':
        return `Sheet: ${settings?.sheetName || 'Sheet1'}`;

      case 'send-email-notification':
        return `To: ${(settings?.recipients || '').split(',')[0] || 'N/A'}`;

      case 'rust-deduplicate':
        return `Dedup by: ${settings?.field || 'N/A'}`;

      case 'rust-filter':
        return `${settings?.field || 'field'} ${settings?.operator || '=='} ${settings?.value ?? ''}`;

      case 'rust-sort':
        return `Sort by: ${settings?.field || 'N/A'} ${settings?.descending ? '↓' : '↑'}`;

      case 'rust-limit':
        return `Limit: ${settings?.count ?? 'N/A'} (offset: ${settings?.offset ?? 0})`;

      default:
        return settings?.processorType
          ? `Type: ${settings.processorType}`
          : 'No configuration summary available.';
    }
  };

  const fullSummary = () => {
    switch (data.processorType) {
      case 'save-to-database':
        return `${settings?.user || ''}@${settings?.host || ''}/${settings?.database || ''} -> ${settings?.tableName || ''}`;
      case 'send-to-api':
        return `${settings?.method || 'POST'} to ${settings?.endpointUrl || ''}`;
      default:
        return renderSummary();
    }
  };

  return (
    <BaseNode title="Processor" icon={<Cog6ToothIcon />} selected={selected} bgColorClass="bg-slate-100">
      <div className="p-2 text-center bg-white/50 rounded-md">
        <p className="text-sm font-semibold text-gray-800">
          {processorName}
        </p>
        <p className="text-xs text-gray-600 mt-1 font-mono break-all" title={fullSummary()}>
          {renderSummary()}
        </p>
        <p className="text-xs text-gray-500 mt-2 italic">
          Handles the processing of mapped data.
        </p>
      </div>
    </BaseNode>
  );
};

export default memo(ProcessorNode);