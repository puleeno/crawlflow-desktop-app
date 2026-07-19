// Realtime WebSocket client for the CrawlFlow headless service.
//
// The background service runs as a separate OS process with no Tauri
// AppHandle, so it cannot emit Tauri events to the GUI. Instead it runs a tiny
// WebSocket server per project and pushes progress / logs / per-item events
// live. This module connects to that server and dispatches frames to handlers.
//
// The WS port for a project is discovered from `get_service_status_cmd`
// (ServiceInfo.ws_port). We keep the Tauri `service-status:*` event as a
// control-plane fallback (e.g. to learn the port and the running status), but
// the continuous data stream now arrives over the socket with no polling delay.

export type WsFrame =
  | { type: 'progress'; payload: any }
  | { type: 'log'; payload: any }
  | { type: 'item'; payload: any }
  | { type: 'status'; payload: any }
  | { type: 'hello'; payload: any }
  | { type: string; payload: any };

export interface WsHandlers {
  onProgress?: (payload: any) => void;
  onLog?: (payload: any) => void;
  onItem?: (payload: any) => void;
  onStatus?: (payload: any) => void;
  onFrame?: (frame: WsFrame) => void;
  onClose?: () => void;
  onOpen?: () => void;
}

export class ProjectWsClient {
  private ws: WebSocket | null = null;
  private port: number = 0;
  private handlers: WsHandlers;
  private reconnectTimer: any = null;
  private closedByUser: boolean = false;
  private projectId: string;

  constructor(projectId: string, handlers: WsHandlers) {
    this.projectId = projectId;
    this.handlers = handlers;
  }

  /** Connect to a discovered port. Pass 0 to disconnect. */
  connect(port: number) {
    if (port === 0) {
      this.disconnect();
      return;
    }
    if (this.port === port && this.ws && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      return; // already connected to this port
    }
    this.disconnect();
    this.port = port;
    this.closedByUser = false;
    this.openSocket();
  }

  private openSocket() {
    if (this.port === 0) return;
    try {
      this.ws = new WebSocket(`ws://127.0.0.1:${this.port}`);
    } catch (e) {
      console.warn('[WS] failed to construct socket', e);
      this.scheduleReconnect();
      return;
    }

    this.ws.onopen = () => {
      this.handlers.onOpen?.();
    };

    this.ws.onmessage = (ev) => {
      let frame: WsFrame;
      try {
        frame = JSON.parse(ev.data as string);
      } catch {
        return;
      }
      this.handlers.onFrame?.(frame);
      switch (frame.type) {
        case 'progress':
          this.handlers.onProgress?.(frame.payload);
          break;
        case 'log':
          this.handlers.onLog?.(frame.payload);
          break;
        case 'item':
          this.handlers.onItem?.(frame.payload);
          break;
        case 'status':
          this.handlers.onStatus?.(frame.payload);
          break;
      }
    };

    this.ws.onclose = () => {
      this.handlers.onClose?.();
      if (!this.closedByUser) this.scheduleReconnect();
    };

    this.ws.onerror = () => {
      // onclose will follow and handle reconnect
    };
  }

  private scheduleReconnect() {
    if (this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      if (!this.closedByUser) this.openSocket();
    }, 2000);
  }

  disconnect() {
    this.closedByUser = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.ws) {
      try { this.ws.close(); } catch { /* ignore */ }
      this.ws = null;
    }
  }
}

/**
 * Resolve the WS port for a project via the existing Tauri command.
 * Returns 0 when the service is not running / has no WS server.
 */
export async function getProjectWsPort(projectId: string): Promise<number> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const info: any = await invoke('get_service_status_cmd', { projectId });
    return info?.ws_port || 0;
  } catch {
    return 0;
  }
}
