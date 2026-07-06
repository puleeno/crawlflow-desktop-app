import React, { useState, useEffect, useCallback } from 'react';
import { Cog6ToothIcon, PlayIcon, StopIcon, XMarkIcon } from './icons';

interface ServiceInstallInfo {
  installed: boolean;
  running: boolean;
  auto_start: boolean;
  service_path: string;
  platform: string;
  executable: string;
  log_dir: string;
}

interface AppSettingsProps {
  onClose: () => void;
}

const AppSettings: React.FC<AppSettingsProps> = ({ onClose }) => {
  const [serviceInfo, setServiceInfo] = useState<ServiceInstallInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [operating, setOperating] = useState(false);
  const [message, setMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);
  const [chromePath, setChromePath] = useState('');
  const [savedChromePath, setSavedChromePath] = useState('');
  const [chromeLoading, setChromeLoading] = useState(false);
  const [chromeMessage, setChromeMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const fetchInfo = useCallback(async () => {
    setLoading(true);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const info: ServiceInstallInfo = await invoke('get_service_install_info_cmd');
      setServiceInfo(info);
    } catch (e) {
      setServiceInfo(null);
    }
    setLoading(false);
  }, []);

  const fetchChromePath = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const path: string | null = await invoke('get_app_setting_cmd', { key: 'chrome_path' });
      const val = path ?? '';
      setChromePath(val);
      setSavedChromePath(val);
    } catch (e) { /* ignore */ }
  }, []);

  const handleSaveChromePath = async () => {
    setChromeLoading(true);
    setChromeMessage(null);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('set_app_setting_cmd', { key: 'chrome_path', value: chromePath });
      setSavedChromePath(chromePath);
      setChromeMessage({ type: 'success', text: 'Chrome path saved.' });
    } catch (e: any) {
      setChromeMessage({ type: 'error', text: e?.toString() || 'Failed to save' });
    }
    setChromeLoading(false);
  };

  useEffect(() => {
    fetchInfo();
    fetchChromePath();
  }, [fetchInfo, fetchChromePath]);

  const showMsg = (type: 'success' | 'error', text: string) => {
    setMessage({ type, text });
    setTimeout(() => setMessage(null), 5000);
  };

  const handleInstall = async () => {
    setOperating(true);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const result: string = await invoke('install_system_service_cmd');
      showMsg('success', result);
      await fetchInfo();
    } catch (e: any) {
      showMsg('error', e?.toString() || 'Installation failed');
    }
    setOperating(false);
  };

  const handleUninstall = async () => {
    setOperating(true);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const result: string = await invoke('uninstall_system_service_cmd');
      showMsg('success', result);
      await fetchInfo();
    } catch (e: any) {
      showMsg('error', e?.toString() || 'Uninstall failed');
    }
    setOperating(false);
  };

  const handleStart = async () => {
    setOperating(true);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const result: string = await invoke('start_system_service_cmd');
      showMsg('success', result);
      await fetchInfo();
    } catch (e: any) {
      showMsg('error', e?.toString() || 'Start failed');
    }
    setOperating(false);
  };

  const handleStop = async () => {
    setOperating(true);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const result: string = await invoke('stop_system_service_cmd');
      showMsg('success', result);
      await fetchInfo();
    } catch (e: any) {
      showMsg('error', e?.toString() || 'Stop failed');
    }
    setOperating(false);
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-50 to-slate-100">
      <div className="max-w-3xl mx-auto px-4 py-8">
        {/* Header */}
        <div className="flex items-center justify-between mb-8">
          <div className="flex items-center gap-3">
            <div className="p-2.5 bg-indigo-600 rounded-xl shadow-lg text-white">
              <Cog6ToothIcon />
            </div>
            <div>
              <h1 className="text-2xl font-bold text-gray-900">Settings</h1>
              <p className="text-sm text-gray-500">Configure CrawlFlow system service</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-2 text-gray-500 hover:text-gray-700 hover:bg-slate-200 rounded-lg transition-colors"
          >
            <XMarkIcon />
          </button>
        </div>

        {/* Message */}
        {message && (
          <div className={`mb-6 px-4 py-3 rounded-xl border text-sm font-medium ${
            message.type === 'success'
              ? 'bg-green-50 border-green-200 text-green-700'
              : 'bg-red-50 border-red-200 text-red-700'
          }`}>
            {message.text}
          </div>
        )}

        {/* Prominent Services Section */}
        <div className="bg-white rounded-2xl shadow-sm border border-slate-200 overflow-hidden mb-6">
          <div className="px-6 py-5 border-b border-slate-100 bg-gradient-to-r from-indigo-50 to-slate-50">
            <div className="flex items-center gap-3">
              <div className="p-2 bg-indigo-100 rounded-lg">
                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor" className="w-6 h-6 text-indigo-600">
                  <path strokeLinecap="round" strokeLinejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456z" />
                </svg>
              </div>
              <div>
                <h2 className="text-lg font-bold text-gray-900">System Service</h2>
                <p className="text-sm text-gray-500">
                  Run CrawlFlow as a background system service that starts automatically with your computer
                </p>
              </div>
            </div>
          </div>

          {loading ? (
            <div className="flex justify-center py-12">
              <div className="animate-spin w-8 h-8 border-4 border-indigo-600 border-t-transparent rounded-full" />
            </div>
          ) : !serviceInfo ? (
            <div className="px-6 py-8 text-center text-gray-500">
              <p>Could not check service status. Are you running in Tauri?</p>
            </div>
          ) : (
            <div className="p-6 space-y-5">
              {/* Status Cards */}
              <div className="grid grid-cols-3 gap-4">
                <div className="bg-slate-50 rounded-xl p-4 border border-slate-200">
                  <div className="text-xs font-semibold text-gray-500 uppercase tracking-wide mb-1">Status</div>
                  <div className="flex items-center gap-2">
                    <span className={`inline-block w-3 h-3 rounded-full ${
                      serviceInfo.running ? 'bg-green-500 animate-pulse' :
                      serviceInfo.installed ? 'bg-amber-400' : 'bg-gray-300'
                    }`} />
                    <span className="font-bold text-gray-800">
                      {serviceInfo.running ? 'Running' : serviceInfo.installed ? 'Stopped' : 'Not Installed'}
                    </span>
                  </div>
                </div>
                <div className="bg-slate-50 rounded-xl p-4 border border-slate-200">
                  <div className="text-xs font-semibold text-gray-500 uppercase tracking-wide mb-1">Platform</div>
                  <span className="font-bold text-gray-800">{serviceInfo.platform}</span>
                </div>
                <div className="bg-slate-50 rounded-xl p-4 border border-slate-200">
                  <div className="text-xs font-semibold text-gray-500 uppercase tracking-wide mb-1">Auto Start</div>
                  <span className={`font-bold ${serviceInfo.auto_start ? 'text-green-600' : 'text-gray-400'}`}>
                    {serviceInfo.auto_start ? 'Enabled' : 'Disabled'}
                  </span>
                </div>
              </div>

              {/* Executable Path */}
              <div className="bg-slate-50 rounded-xl p-4 border border-slate-200">
                <div className="text-xs font-semibold text-gray-500 uppercase tracking-wide mb-1">Executable</div>
                <div className="text-sm font-mono text-gray-700 truncate" title={serviceInfo.executable}>
                  {serviceInfo.executable || 'N/A'}
                </div>
              </div>

              {/* Service File Location */}
              <div className="bg-slate-50 rounded-xl p-4 border border-slate-200">
                <div className="text-xs font-semibold text-gray-500 uppercase tracking-wide mb-1">Service File</div>
                <div className="text-sm font-mono text-gray-700 truncate" title={serviceInfo.service_path}>
                  {serviceInfo.service_path || 'N/A'}
                </div>
              </div>

              {/* Log Directory */}
              <div className="bg-slate-50 rounded-xl p-4 border border-slate-200">
                <div className="text-xs font-semibold text-gray-500 uppercase tracking-wide mb-1">Log Directory</div>
                <div className="text-sm font-mono text-gray-700 truncate" title={serviceInfo.log_dir}>
                  {serviceInfo.log_dir || 'N/A'}
                </div>
              </div>

              {/* Actions */}
              <div className="flex flex-wrap gap-3 pt-2">
                {!serviceInfo.installed ? (
                  <button
                    onClick={handleInstall}
                    disabled={operating}
                    className="flex items-center gap-2 px-5 py-2.5 text-sm font-bold text-white bg-indigo-600 hover:bg-indigo-700 disabled:opacity-50 rounded-xl shadow-sm transition-colors"
                  >
                    {operating ? (
                      <div className="animate-spin w-4 h-4 border-2 border-white border-t-transparent rounded-full" />
                    ) : (
                      <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor" className="w-4 h-4">
                        <path strokeLinecap="round" strokeLinejoin="round" d="M12 4.5v15m7.5-7.5h-15" />
                      </svg>
                    )}
                    Install Service
                  </button>
                ) : (
                  <>
                    {!serviceInfo.running ? (
                      <button
                        onClick={handleStart}
                        disabled={operating}
                        className="flex items-center gap-2 px-5 py-2.5 text-sm font-bold text-white bg-green-600 hover:bg-green-700 disabled:opacity-50 rounded-xl shadow-sm transition-colors"
                      >
                        <PlayIcon size={16} /> Start Service
                      </button>
                    ) : (
                      <button
                        onClick={handleStop}
                        disabled={operating}
                        className="flex items-center gap-2 px-5 py-2.5 text-sm font-bold text-white bg-amber-600 hover:bg-amber-700 disabled:opacity-50 rounded-xl shadow-sm transition-colors"
                      >
                        <StopIcon size={16} /> Stop Service
                      </button>
                    )}
                    <button
                      onClick={handleUninstall}
                      disabled={operating}
                      className="flex items-center gap-2 px-5 py-2.5 text-sm font-bold text-red-700 bg-red-50 hover:bg-red-100 border border-red-200 disabled:opacity-50 rounded-xl transition-colors"
                    >
                      Uninstall Service
                    </button>
                  </>
                )}
              </div>
            </div>
          )}
        </div>

        {/* Chrome Browser Path */}
        <div className="bg-white rounded-2xl shadow-sm border border-slate-200 p-6">
          <h3 className="text-base font-bold text-gray-800 mb-4">Chrome Browser</h3>
          <div className="space-y-3">
            <label className="text-sm font-semibold text-gray-700">Chrome/Chromium Path</label>
            <p className="text-xs text-gray-500">
              Leave empty to auto-detect. Set a custom path if Chrome is not found automatically.
            </p>
            <div className="flex gap-2">
              <input
                type="text"
                value={chromePath}
                onChange={e => setChromePath(e.target.value)}
                placeholder="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
                className="flex-1 p-2.5 text-sm border border-slate-300 rounded-xl bg-white focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
              />
              <button
                onClick={handleSaveChromePath}
                disabled={chromePath === savedChromePath || chromeLoading}
                className="px-4 py-2.5 text-sm font-bold text-white bg-indigo-600 hover:bg-indigo-700 disabled:opacity-50 rounded-xl transition-colors"
              >
                Save
              </button>
            </div>
            {chromeMessage && (
              <p className={`text-xs font-medium ${chromeMessage.type === 'success' ? 'text-green-600' : 'text-red-600'}`}>
                {chromeMessage.text}
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default AppSettings;
