import React, { useState, useEffect, useRef, useCallback } from 'react';
import { PlayIcon, StopIcon, PauseIcon, XMarkIcon, ChevronDownIcon, ChevronUpIcon } from './icons';

interface LogEntry {
  id: number;
  project_id: string;
  timestamp: string;
  level: string;
  source: string;
  message: string;
  details?: string | null;
}

interface LiveLogsProps {
  projectId: string;
  onClose: () => void;
}

const LEVEL_COLORS: Record<string, string> = {
  error: 'text-red-600 bg-red-50 border-red-200',
  warn: 'text-amber-600 bg-amber-50 border-amber-200',
  info: 'text-blue-600 bg-blue-50 border-blue-200',
  debug: 'text-gray-500 bg-gray-50 border-gray-200',
};

const LEVEL_BADGE: Record<string, string> = {
  error: 'bg-red-100 text-red-700',
  warn: 'bg-amber-100 text-amber-700',
  info: 'bg-blue-100 text-blue-700',
  debug: 'bg-gray-100 text-gray-600',
};

const LiveLogs: React.FC<LiveLogsProps> = ({ projectId, onClose }) => {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [levelFilter, setLevelFilter] = useState<string>('all');
  const [autoScroll, setAutoScroll] = useState(true);
  const [isExpanded, setIsExpanded] = useState(true);
  const [serviceStatus, setServiceStatus] = useState<string>('stopped');
  const [serviceInfo, setServiceInfo] = useState<any>(null);
  const logEndRef = useRef<HTMLDivElement>(null);
  const listenerRef = useRef<(() => void) | null>(null);

  // Subscribe to live log events
  useEffect(() => {
    const logEvent = `project-log:${projectId}`;
    const statusEvent = `service-status:${projectId}`;

    let unlisten: (() => void) | null = null;
    let unlistenStatus: (() => void) | null = null;

    const setup = async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');

        unlisten = await listen<LogEntry>(logEvent, (event) => {
          setLogs(prev => {
            const next = [...prev, event.payload];
            if (next.length > 500) next.splice(0, next.length - 500);
            return next;
          });
        });

        unlistenStatus = await listen<any>(statusEvent, (event) => {
          const payload = event.payload;
          setServiceStatus(payload.status || 'stopped');
          setServiceInfo(payload);
        });
      } catch (e) {
        // Not in Tauri environment, skip live events
      }
    };

    setup();

    return () => {
      if (unlisten) unlisten();
      if (unlistenStatus) unlistenStatus();
    };
  }, [projectId]);

  // Auto-scroll
  useEffect(() => {
    if (autoScroll && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [logs.length, autoScroll]);

  const filteredLogs = logs.filter(l => levelFilter === 'all' || l.level === levelFilter);

  const handleStartService = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('start_project_service_cmd', {
        projectId,
        nodes: [],
        edges: [],
        settings: { intervalSeconds: 60 },
      });
    } catch (e) {
      console.error('Failed to start service:', e);
    }
  }, [projectId]);

  const handleStopService = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('stop_project_service_cmd', { projectId });
    } catch (e) {
      console.error('Failed to stop service:', e);
    }
  }, [projectId]);

  const handlePauseService = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('pause_project_service_cmd', { projectId });
    } catch (e) {
      console.error('Failed to pause service:', e);
    }
  }, [projectId]);

  const handleResumeService = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('resume_project_service_cmd', { projectId });
    } catch (e) {
      console.error('Failed to resume service:', e);
    }
  }, [projectId]);

  const clearLogs = useCallback(async () => {
    setLogs([]);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('clear_project_logs_cmd', { projectId });
    } catch (e) { /* ignore */ }
  }, [projectId]);

  const isRunning = serviceStatus === 'running';
  const isPaused = serviceStatus === 'paused';
  const isError = serviceStatus?.startsWith('error');
  const isStopped = !isRunning && !isPaused && !isError;

  const statusColor = isRunning ? 'bg-green-500' : isPaused ? 'bg-amber-500' : isError ? 'bg-red-500' : 'bg-gray-400';
  const statusLabel = isRunning ? 'Running' : isPaused ? 'Paused' : isError ? 'Error' : 'Stopped';

  return (
    <div className="border-t border-slate-200 bg-white shadow-inner">
      {/* Header bar */}
      <div className="flex items-center justify-between px-4 py-2 bg-slate-50 border-b border-slate-200">
        <div className="flex items-center gap-3">
          <button onClick={() => setIsExpanded(!isExpanded)} className="p-1 text-gray-500 hover:text-gray-700">
            {isExpanded ? <ChevronDownIcon /> : <ChevronUpIcon />}
          </button>
          <div className="flex items-center gap-2">
            <span className={`inline-block w-2.5 h-2.5 rounded-full ${statusColor} animate-pulse`} />
            <span className="text-sm font-semibold text-gray-700">Service</span>
            <span className={`text-xs font-medium px-2 py-0.5 rounded-full ${
              isRunning ? 'bg-green-100 text-green-700' :
              isPaused ? 'bg-amber-100 text-amber-700' :
              isError ? 'bg-red-100 text-red-700' :
              'bg-gray-100 text-gray-600'
            }`}>{statusLabel}</span>
          </div>
          {serviceInfo && (
            <span className="text-xs text-gray-400">Cycle #{serviceInfo.cycle_count}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {isStopped && (
            <button onClick={handleStartService} className="flex items-center gap-1 px-2.5 py-1 text-xs font-semibold text-white bg-green-600 hover:bg-green-700 rounded-lg transition-colors">
              <PlayIcon size={14} /> Start
            </button>
          )}
          {isRunning && (
            <>
              <button onClick={handlePauseService} className="flex items-center gap-1 px-2.5 py-1 text-xs font-semibold text-amber-700 bg-amber-100 hover:bg-amber-200 rounded-lg transition-colors">
                <PauseIcon size={14} /> Pause
              </button>
              <button onClick={handleStopService} className="flex items-center gap-1 px-2.5 py-1 text-xs font-semibold text-red-700 bg-red-100 hover:bg-red-200 rounded-lg transition-colors">
                <StopIcon size={14} /> Stop
              </button>
            </>
          )}
          {isPaused && (
            <>
              <button onClick={handleResumeService} className="flex items-center gap-1 px-2.5 py-1 text-xs font-semibold text-green-700 bg-green-100 hover:bg-green-200 rounded-lg transition-colors">
                <PlayIcon size={14} /> Resume
              </button>
              <button onClick={handleStopService} className="flex items-center gap-1 px-2.5 py-1 text-xs font-semibold text-red-700 bg-red-100 hover:bg-red-200 rounded-lg transition-colors">
                <StopIcon size={14} /> Stop
              </button>
            </>
          )}
          {isError && (
            <button onClick={handleStopService} className="flex items-center gap-1 px-2.5 py-1 text-xs font-semibold text-red-700 bg-red-100 hover:bg-red-200 rounded-lg transition-colors">
              <StopIcon size={14} /> Stop
            </button>
          )}
          <select
            value={levelFilter}
            onChange={e => setLevelFilter(e.target.value)}
            className="text-xs border border-slate-300 rounded-lg px-2 py-1 bg-white text-gray-700"
          >
            <option value="all">All Levels</option>
            <option value="error">Error</option>
            <option value="warn">Warning</option>
            <option value="info">Info</option>
            <option value="debug">Debug</option>
          </select>
          <button onClick={clearLogs} className="px-2 py-1 text-xs font-medium text-gray-500 hover:text-gray-700 hover:bg-slate-100 rounded-lg">Clear</button>
          <button onClick={() => setAutoScroll(!autoScroll)} className={`px-2 py-1 text-xs font-medium rounded-lg ${autoScroll ? 'text-indigo-600 bg-indigo-50' : 'text-gray-500 hover:text-gray-700'}`}>
            Auto-scroll
          </button>
          <button onClick={onClose} className="p-1 text-gray-400 hover:text-gray-700">
            <XMarkIcon />
          </button>
        </div>
      </div>

      {/* Log entries */}
      {isExpanded && (
        <div className="overflow-y-auto" style={{ maxHeight: '30vh' }}>
          {filteredLogs.length === 0 ? (
            <div className="flex items-center justify-center h-24 text-sm text-gray-400">
              No logs yet. Start the service to see pipeline execution logs.
            </div>
          ) : (
            filteredLogs.map((log) => (
              <div key={log.id} className={`px-4 py-1.5 border-b border-slate-100 text-xs font-mono ${LEVEL_COLORS[log.level] || 'text-gray-600'}`}>
                <div className="flex items-start gap-2">
                  <span className={`inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-bold uppercase ${LEVEL_BADGE[log.level] || 'bg-gray-100 text-gray-600'}`}>
                    {log.level}
                  </span>
                  <span className="text-gray-400 shrink-0">{log.timestamp}</span>
                  <span className="text-gray-500 shrink-0">[{log.source}]</span>
                  <span className="flex-1">{log.message}</span>
                </div>
                {log.details && (
                  <div className="mt-0.5 ml-14 text-gray-400 truncate" title={log.details}>
                    {log.details}
                  </div>
                )}
              </div>
            ))
          )}
          <div ref={logEndRef} />
        </div>
      )}
    </div>
  );
};

export default LiveLogs;
