/**
 * ServiceControls — communicates exclusively with the background service via SQLite.
 * Status is polled every POLL_INTERVAL_MS from `get_service_status_cmd`.
 * Start/Stop write to project_runtime via request_project_run/stop_cmd.
 * The background `crawlflow-service` binary is the ONLY executor.
 */
import React, { useState, useEffect, useCallback, useRef } from 'react';
import { PlayIcon, StopIcon } from './icons';
import type { Node, Edge } from 'reactflow';

interface ServiceControlsProps {
  projectId: string;
  onOpenLogs: () => void;
  nodes?: Node[];
  edges?: Edge[];
  /** Provided by App.tsx as the single source of truth (already being polled) */
  externalStatus?: string;
  externalCycleCount?: number;
}

const ServiceControls: React.FC<ServiceControlsProps> = ({
  projectId,
  onOpenLogs,
  externalStatus,
  externalCycleCount,
}) => {
  const [localStatus, setLocalStatus] = useState<string>('stopped');
  const [localCycleCount, setLocalCycleCount] = useState<number>(0);
  const [lastRunAt, setLastRunAt] = useState<string>('');
  const [lastError, setLastError] = useState<string | null>(null);
  const [intervalSec, setIntervalSec] = useState<number>(60);
  const [busy, setBusy] = useState(false);

  // Use external status when provided (App.tsx polls), otherwise manage locally
  const status = externalStatus ?? localStatus;
  const cycleCount = externalCycleCount ?? localCycleCount;

  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const poll = useCallback(async () => {
    try {
        const { invoke } = await import('../lib/platform');
      const info: any = await invoke('get_service_status_cmd', { projectId });
      if (info) {
        if (externalStatus === undefined) {
          setLocalStatus(info.status ?? 'stopped');
          setLocalCycleCount(info.cycle_count ?? 0);
        }
        setLastRunAt(info.last_run_at ?? '');
        setLastError(info.last_error ?? null);
        setIntervalSec(info.interval_seconds ?? 60);
      }
    } catch (_) { /* not in tauri */ }
  }, [projectId, externalStatus]);

  // Realtime: subscribe to the per-project status event emitted by the GUI
  // process. Keeps ancillary info (lastRunAt/error/interval) fresh without
  // relying solely on the slow poll fallback.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    const setup = async () => {
      try {
        const { listen } = await import('../lib/platform');
        unlisten = await listen<any>(`service-status:${projectId}`, (event) => {
          const info = event.payload;
          if (info) {
            if (externalStatus === undefined) {
              setLocalStatus(info.status ?? 'stopped');
              setLocalCycleCount(info.cycle_count ?? 0);
            }
            setLastRunAt(info.last_run_at ?? '');
            setLastError(info.last_error ?? null);
            setIntervalSec(info.interval_seconds ?? 60);
          }
        });
      } catch (_) { /* not in tauri */ }
    };
    setup();
    return () => { if (unlisten) unlisten(); };
  }, [projectId, externalStatus]);

  // When using external status, still poll for ancillary info (lastRunAt, error)
  // When standalone, poll for everything. Slow interval — events are primary.
  useEffect(() => {
    poll(); // immediate
    pollRef.current = setInterval(poll, 15000); // fallback only
    return () => { if (pollRef.current) clearInterval(pollRef.current); };
  }, [poll]);

  const requestStart = useCallback(async () => {
    setBusy(true);
    try {
        const { invoke } = await import('../lib/platform');
      await invoke('request_project_run_cmd', { projectId });
      // Optimistically show pending while we wait for background service first cycle
      if (externalStatus === undefined) setLocalStatus('idle');
      // Force an immediate poll after a short delay
      setTimeout(poll, 500);
    } catch (e: any) { console.error(e); }
    finally { setBusy(false); }
  }, [projectId, externalStatus, poll]);

  const requestStop = useCallback(async () => {
    setBusy(true);
    try {
        const { invoke } = await import('../lib/platform');
      await invoke('request_project_stop_cmd', { projectId });
      if (externalStatus === undefined) setLocalStatus('stopped');
      setTimeout(poll, 500);
    } catch (e: any) { console.error(e); }
    finally { setBusy(false); }
  }, [projectId, externalStatus, poll]);

  const isRunning  = status === 'running';
  const isIdle     = status === 'idle';
  const isError    = status?.startsWith('error');
  const isStopped  = status === 'stopped' || status === 'paused';
  const isPaused   = status === 'paused';

  // Running or idle both mean background service is active for this project
  const serviceActive = isRunning || isIdle;

  const statusDot   = isRunning ? 'bg-green-500' : isIdle ? 'bg-blue-400' : isError ? 'bg-red-500' : 'bg-gray-400';
  const statusBg    = isRunning ? 'bg-green-50 border-green-200' : isIdle ? 'bg-blue-50 border-blue-200' : isError ? 'bg-red-50 border-red-200' : 'bg-gray-50 border-gray-200';
  const statusLabel = isRunning ? 'Running' : isIdle ? 'Idle' : isError ? 'Error' : isPaused ? 'Paused' : 'Stopped';

  return (
    <div className={`rounded-xl border p-4 ${statusBg}`}>
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <span className={`inline-block w-3 h-3 rounded-full ${statusDot} ${isRunning ? 'animate-pulse' : ''}`} />
          <span className="font-semibold text-gray-800">Service</span>
          <span className={`text-xs font-bold px-2 py-0.5 rounded-full ${
            isRunning       ? 'bg-green-200 text-green-800' :
            isIdle          ? 'bg-blue-200 text-blue-800' :
            isError         ? 'bg-red-200 text-red-800' :
                              'bg-gray-200 text-gray-700'
          }`}>{statusLabel}</span>
        </div>
        <div className="flex items-center gap-1.5">
          {isStopped && (
            <button
              onClick={requestStart}
              disabled={busy}
              className="flex items-center gap-1 px-3 py-1.5 text-xs font-bold text-white bg-green-600 hover:bg-green-700 disabled:opacity-50 rounded-lg transition-colors"
            >
              <PlayIcon size={14} /> Start
            </button>
          )}
          {serviceActive && (
            <button
              onClick={requestStop}
              disabled={busy}
              className="flex items-center gap-1 px-3 py-1.5 text-xs font-bold text-red-700 bg-red-100 hover:bg-red-200 disabled:opacity-50 rounded-lg transition-colors"
            >
              <StopIcon size={14} /> Stop
            </button>
          )}
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-3 gap-3 text-xs">
        <div className="bg-white rounded-lg p-2 border border-slate-200">
          <span className="text-gray-500">Cycles</span>
          <div className="font-bold text-gray-800 mt-0.5">{cycleCount}</div>
        </div>
        <div className="bg-white rounded-lg p-2 border border-slate-200">
          <span className="text-gray-500">Interval</span>
          <div className="font-bold text-gray-800 mt-0.5">{intervalSec}s</div>
        </div>
        <div className="bg-white rounded-lg p-2 border border-slate-200">
          <span className="text-gray-500">Last Run</span>
          <div className="font-bold text-gray-800 mt-0.5 truncate" title={lastRunAt}>
            {lastRunAt ? new Date(lastRunAt).toLocaleTimeString() : '--'}
          </div>
        </div>
      </div>

      {isError && lastError && (
        <div className="mt-2 text-xs text-red-600 bg-red-50 border border-red-200 rounded-lg p-2">
          {lastError}
        </div>
      )}

      <div className="mt-2 text-xs text-gray-400 text-center">
        Run by background service · realtime status
      </div>

      <button
        onClick={onOpenLogs}
        className="mt-3 w-full py-2 text-xs font-semibold text-indigo-600 bg-indigo-50 hover:bg-indigo-100 border border-indigo-200 rounded-lg transition-colors"
      >
        View Live Logs
      </button>
    </div>
  );
};

export default ServiceControls;
