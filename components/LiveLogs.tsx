import React, { useState, useEffect, useRef, useCallback } from 'react';
import { XMarkIcon, ChevronDownIcon, ChevronUpIcon } from './icons';
import { ProjectWsClient } from '../wsClient';

interface LogEntry {
  id: number;
  project_id: string;
  timestamp: string;
  level: string;
  source: string;
  message: string;
  details?: string | null;
}

interface ProgressPayload {
  items_total?: number;
  items_processed?: number;
  items_success?: number;
  items_failed?: number;
  items_pending?: number;
  progress_pct?: number;
  phase?: string;
  message?: string;
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
  const [progress, setProgress] = useState<ProgressPayload | null>(null);
  const logEndRef = useRef<HTMLDivElement>(null);
  const seenRef = useRef<Set<string>>(new Set());
  const pollLastIdRef = useRef(0);
  const wsRef = useRef<ProjectWsClient | null>(null);
  const wsConnectedRef = useRef(false);

  // Fetch existing logs from DB on mount (captures background service logs)
  useEffect(() => {
    seenRef.current = new Set();
    (async () => {
      try {
        const { invoke } = await import('../lib/platform');
        const existing = await invoke<LogEntry[]>('get_project_logs_cmd', {
          projectId,
          sinceId: null,
          levelFilter: null,
          limit: 500,
        });
        if (existing.length > 0) {
          for (const l of existing) seenRef.current.add(`${l.timestamp}|${l.message}`);
          setLogs(existing);
          pollLastIdRef.current = existing.reduce((max, l) => Math.max(max, l.id), 0);
        }
      } catch (e) {
        // Not in Tauri or DB not available
      }
    })();
  }, [projectId]);

  // Subscribe to Tauri events (in-process logs + status)
  useEffect(() => {
    const logEvent = `project-log:${projectId}`;
    const statusEvent = `service-status:${projectId}`;

    const unlistenRef: { current: (() => void) | null } = { current: null };
    const unlistenStatusRef: { current: (() => void) | null } = { current: null };
    let cancelled = false;

    const setup = async () => {
      try {
        const { listen } = await import('../lib/platform');

        const unsub1 = await listen<LogEntry>(logEvent, (event) => {
          const l = event.payload;
          const key = `${l.timestamp}|${l.message}`;
          if (seenRef.current.has(key)) return;
          seenRef.current.add(key);
          setLogs(prev => {
            const next = [...prev, l];
            if (next.length > 500) next.splice(0, next.length - 500);
            return next;
          });
        });

        const unsub2 = await listen<any>(statusEvent, (event) => {
          const payload = event.payload;
          setServiceStatus(payload.status || 'stopped');
          setServiceInfo(payload);
        });

        if (cancelled) {
          unsub1();
          unsub2();
        } else {
          unlistenRef.current = unsub1;
          unlistenStatusRef.current = unsub2;
        }
      } catch (e) {
        // Not in Tauri environment, skip live events
      }
    };

    setup();

    return () => {
      cancelled = true;
      unlistenRef.current?.();
      unlistenStatusRef.current?.();
    };
  }, [projectId]);

  // Connect to the background service's WebSocket for realtime logs + progress
  useEffect(() => {
    let cancelled = false;

    const connectWs = async () => {
      try {
        const { invoke } = await import('../lib/platform');
        const info: any = await invoke('get_service_status_cmd', { projectId });
        if (cancelled || !info?.ws_port) return;

        setServiceStatus(info.status || 'stopped');
        setServiceInfo(info);

        const ws = new ProjectWsClient(projectId, {
          onLog: (payload) => {
            if (!payload || !payload.message) return;
            const key = `${payload.timestamp}|${payload.message}`;
            if (seenRef.current.has(key)) return;
            seenRef.current.add(key);
            setLogs(prev => {
              const next = [...prev, {
                id: payload.id || Date.now(),
                project_id: projectId,
                timestamp: payload.timestamp || '',
                level: payload.level || 'info',
                source: payload.source || 'ws',
                message: payload.message,
                details: payload.details || null,
              }];
              if (next.length > 500) next.splice(0, next.length - 500);
              return next;
            });
          },
          onProgress: (payload) => {
            setProgress(payload);
          },
          onStatus: (payload) => {
            if (payload?.status) setServiceStatus(payload.status);
          },
          onOpen: () => { wsConnectedRef.current = true; },
          onClose: () => { wsConnectedRef.current = false; },
        });
        wsRef.current = ws;
        ws.connect(info.ws_port);
      } catch { /* ignore */ }
    };

    connectWs();

    // Periodically reconnect if WS dropped
    const reconnectTimer = setInterval(connectWs, 3000);

    return () => {
      cancelled = true;
      clearInterval(reconnectTimer);
      wsRef.current?.disconnect();
      wsRef.current = null;
    };
  }, [projectId]);

  // Poll for new logs from DB every 2s (fallback for headless service mode)
  // This catches logs written by the service but not received via WS (e.g. during startup)
  useEffect(() => {
    let cancelled = false;

    const poll = async () => {
      if (cancelled) return;
      try {
        const { invoke } = await import('../lib/platform');
        const newLogs = await invoke<LogEntry[]>('get_project_logs_cmd', {
          projectId,
          sinceId: pollLastIdRef.current || null,
          levelFilter: null,
          limit: 100,
        });
        if (newLogs.length > 0) {
          for (const l of newLogs) {
            if (l.id > pollLastIdRef.current) pollLastIdRef.current = l.id;
          }
          setLogs(prev => {
            const next = [...prev];
            for (const l of newLogs) {
              const key = `${l.timestamp}|${l.message}`;
              if (seenRef.current.has(key)) continue;
              seenRef.current.add(key);
              next.push(l);
            }
            if (next.length > 500) next.splice(0, next.length - 500);
            return next;
          });
        }
      } catch { /* DB not available */ }
    };

    const timer = setInterval(poll, 500);
    return () => { cancelled = true; clearInterval(timer); };
  }, [projectId]);

  // Auto-scroll
  useEffect(() => {
    if (autoScroll && logEndRef.current) {
      logEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [logs.length, autoScroll]);

  const filteredLogs = logs.filter(l => levelFilter === 'all' || l.level === levelFilter);

  const isRunning = serviceStatus === 'running';
  const isPaused = serviceStatus === 'paused';
  const isError = serviceStatus?.startsWith('error');
  const isStopped = !isRunning && !isPaused && !isError;

  const clearLogs = useCallback(async () => {
    setLogs([]);
    seenRef.current = new Set();
    try {
      const { invoke } = await import('../lib/platform');
      await invoke('clear_project_logs_cmd', { projectId });
    } catch (e) { /* ignore */ }
  }, [projectId]);

  const statusColor = isRunning ? 'bg-green-500' : isPaused ? 'bg-amber-500' : isError ? 'bg-red-500' : 'bg-gray-400';
  const statusLabel = isRunning ? 'Running' : isPaused ? 'Paused' : isError ? 'Error' : 'Stopped';

  const progressPct = progress?.progress_pct ?? 0;
  const itemsProcessed = progress?.items_processed ?? 0;
  const itemsTotal = progress?.items_total ?? 0;
  const itemsFailed = progress?.items_failed ?? 0;

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
            <span className={`text-xs font-medium px-2 py-0.5 rounded-full ${isRunning ? 'bg-green-100 text-green-700' :
                isPaused ? 'bg-amber-100 text-amber-700' :
                  isError ? 'bg-red-100 text-red-700' :
                    'bg-gray-100 text-gray-600'
              }`}>{statusLabel}</span>
          </div>
          {serviceInfo && (
            <span className="text-xs text-gray-400">Cycle #{serviceInfo.cycle_count}</span>
          )}
          {/* Realtime progress display */}
          {itemsTotal > 0 && (
            <div className="flex items-center gap-2 ml-2">
              <div className="w-24 h-1.5 bg-gray-200 rounded-full overflow-hidden">
                <div
                  className="h-full bg-blue-500 rounded-full transition-all duration-300"
                  style={{ width: `${Math.min(progressPct, 100)}%` }}
                />
              </div>
              <span className="text-xs text-gray-500">
                {itemsProcessed}/{itemsTotal}
                {itemsFailed > 0 && <span className="text-red-500 ml-1">({itemsFailed} failed)</span>}
              </span>
              <span className="text-xs text-gray-400">{Math.round(progressPct)}%</span>
            </div>
          )}
          {progress?.message && (
            <span className="text-xs text-gray-400 ml-1">{progress.message}</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          <div className="relative">
            <select
              value={levelFilter}
              onChange={e => setLevelFilter(e.target.value)}
              className="text-xs border border-slate-300 rounded-lg pl-2 pr-6 py-1 bg-white text-gray-700 appearance-none cursor-pointer"
            >
              <option value="all">All Levels</option>
              <option value="error">Error</option>
              <option value="warn">Warning</option>
              <option value="info">Info</option>
              <option value="debug">Debug</option>
            </select>
            <div className="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-1.5 text-gray-400">
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" className="w-3.5 h-3.5">
                <path fillRule="evenodd" d="M5.23 7.21a.75.75 0 011.06.02L10 11.168l3.71-3.938a.75.75 0 111.08 1.04l-4.25 4.5a.75.75 0 01-1.08 0l-4.25-4.5a.75.75 0 01.02-1.06z" clipRule="evenodd" />
              </svg>
            </div>
          </div>
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
