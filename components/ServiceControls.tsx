import React, { useState, useEffect, useCallback } from 'react';
import { PlayIcon, StopIcon, PauseIcon } from './icons';

interface ServiceControlsProps {
  projectId: string;
  onOpenLogs: () => void;
}

const ServiceControls: React.FC<ServiceControlsProps> = ({ projectId, onOpenLogs }) => {
  const [status, setStatus] = useState<string>('stopped');
  const [cycleCount, setCycleCount] = useState<number>(0);
  const [lastRunAt, setLastRunAt] = useState<string>('');
  const [lastError, setLastError] = useState<string | null>(null);
  const [intervalSec, setIntervalSec] = useState<number>(60);

  // Fetch initial status
  useEffect(() => {
    const fetchStatus = async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const info: any = await invoke('get_service_status_cmd', { projectId });
        if (info) {
          setStatus(info.status);
          setCycleCount(info.cycle_count);
          setLastRunAt(info.last_run_at);
          setLastError(info.last_error);
          setIntervalSec(info.interval_seconds);
        }
      } catch (e) { /* not in tauri */ }
    };
    fetchStatus();
  }, [projectId]);

  // Subscribe to status changes
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    const setup = async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event');
        unlisten = await listen<any>(`service-status:${projectId}`, (event) => {
          const p = event.payload;
          setStatus(p.status || 'stopped');
          setCycleCount(p.cycle_count || 0);
          setLastRunAt(p.last_run_at || '');
          setLastError(p.last_error || null);
        });
      } catch (e) { /* ignore */ }
    };
    setup();
    return () => { if (unlisten) unlisten(); };
  }, [projectId]);

  const start = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('start_project_service_cmd', {
        projectId,
        nodes: [],
        edges: [],
        settings: { intervalSeconds: intervalSec },
      });
    } catch (e: any) { console.error(e); }
  }, [projectId, intervalSec]);

  const stop = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('stop_project_service_cmd', { projectId });
    } catch (e: any) { console.error(e); }
  }, [projectId]);

  const pause = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('pause_project_service_cmd', { projectId });
    } catch (e: any) { console.error(e); }
  }, [projectId]);

  const resume = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('resume_project_service_cmd', { projectId });
    } catch (e: any) { console.error(e); }
  }, [projectId]);

  const isRunning = status === 'running';
  const isPaused = status === 'paused';
  const isError = status?.startsWith('error');
  const isStopped = !isRunning && !isPaused && !isError;

  const statusDot = isRunning ? 'bg-green-500' : isPaused ? 'bg-amber-500' : isError ? 'bg-red-500' : 'bg-gray-400';
  const statusBg = isRunning ? 'bg-green-50 border-green-200' : isPaused ? 'bg-amber-50 border-amber-200' : isError ? 'bg-red-50 border-red-200' : 'bg-gray-50 border-gray-200';
  const statusLabel = isRunning ? 'Running' : isPaused ? 'Paused' : isError ? 'Error' : 'Stopped';

  return (
    <div className={`rounded-xl border p-4 ${statusBg}`}>
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <span className={`inline-block w-3 h-3 rounded-full ${statusDot} ${isRunning ? 'animate-pulse' : ''}`} />
          <span className="font-semibold text-gray-800">Service</span>
          <span className={`text-xs font-bold px-2 py-0.5 rounded-full ${
            isRunning ? 'bg-green-200 text-green-800' :
            isPaused ? 'bg-amber-200 text-amber-800' :
            isError ? 'bg-red-200 text-red-800' :
            'bg-gray-200 text-gray-700'
          }`}>{statusLabel}</span>
        </div>
        <div className="flex items-center gap-1.5">
          {isStopped && (
            <button onClick={start} className="flex items-center gap-1 px-3 py-1.5 text-xs font-bold text-white bg-green-600 hover:bg-green-700 rounded-lg transition-colors">
              <PlayIcon size={14} /> Start
            </button>
          )}
          {isRunning && (
            <>
              <button onClick={pause} className="flex items-center gap-1 px-3 py-1.5 text-xs font-bold text-amber-700 bg-amber-100 hover:bg-amber-200 rounded-lg transition-colors">
                <PauseIcon size={14} /> Pause
              </button>
              <button onClick={stop} className="flex items-center gap-1 px-3 py-1.5 text-xs font-bold text-red-700 bg-red-100 hover:bg-red-200 rounded-lg transition-colors">
                <StopIcon size={14} /> Stop
              </button>
            </>
          )}
          {isPaused && (
            <>
              <button onClick={resume} className="flex items-center gap-1 px-3 py-1.5 text-xs font-bold text-green-700 bg-green-100 hover:bg-green-200 rounded-lg transition-colors">
                <PlayIcon size={14} /> Resume
              </button>
              <button onClick={stop} className="flex items-center gap-1 px-3 py-1.5 text-xs font-bold text-red-700 bg-red-100 hover:bg-red-200 rounded-lg transition-colors">
                <StopIcon size={14} /> Stop
              </button>
            </>
          )}
          {isError && (
            <>
              <button onClick={stop} className="flex items-center gap-1 px-3 py-1.5 text-xs font-bold text-red-700 bg-red-100 hover:bg-red-200 rounded-lg transition-colors">
                <StopIcon size={14} /> Stop
              </button>
            </>
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
          <div className="flex items-center gap-1 mt-0.5">
            <input
              type="number"
              min={5}
              max={86400}
              value={intervalSec}
              onChange={e => setIntervalSec(parseInt(e.target.value) || 60)}
              className="w-16 px-1.5 py-0.5 border border-slate-300 rounded text-xs font-bold text-gray-800"
              disabled={isRunning || isPaused}
            />
            <span className="text-gray-500">s</span>
          </div>
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
