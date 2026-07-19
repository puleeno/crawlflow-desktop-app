//! Realtime WebSocket hub for the headless CrawlFlow service.
//!
//! The service runs as a separate OS process with no Tauri `AppHandle`, so it
//! cannot emit Tauri events to the GUI. Instead it runs a tiny WebSocket server
//! (one per project) and pushes progress, logs and per-item events to every
//! connected browser client. This removes the two stacked 1-second polls that
//! previously mediated progress between service → SQLite → GUI.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use futures_util::StreamExt;
use futures_util::SinkExt;

/// Message envelope sent over the wire. Every frame is a JSON object:
/// `{ "type": "progress" | "log" | "item" | "status" | "hello", "payload": ... }`
#[derive(Debug, Clone, serde::Serialize)]
pub struct WsMessage {
    pub r#type: String,
    pub payload: serde_json::Value,
}

impl WsMessage {
    pub fn progress(payload: serde_json::Value) -> Self {
        Self { r#type: "progress".into(), payload }
    }
    pub fn log(payload: serde_json::Value) -> Self {
        Self { r#type: "log".into(), payload }
    }
    pub fn item(payload: serde_json::Value) -> Self {
        Self { r#type: "item".into(), payload }
    }
    pub fn status(payload: serde_json::Value) -> Self {
        Self { r#type: "status".into(), payload }
    }
}

type Tx = broadcast::Sender<String>;

struct ProjectChannel {
    port: u16,
    tx: Tx,
}

/// Global registry of per-project WebSocket channels.
pub struct WsHub {
    channels: std::sync::Mutex<HashMap<String, ProjectChannel>>,
    /// Where to persist ws_port so the GUI can discover it.
    on_port_assigned: std::sync::Mutex<Option<Box<dyn Fn(&str, u16) + Send + Sync>>>,
}

impl WsHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            channels: std::sync::Mutex::new(HashMap::new()),
            on_port_assigned: std::sync::Mutex::new(None),
        })
    }

    /// Register a callback invoked when a project's WS port is allocated,
    /// e.g. to persist it into `project_runtime.ws_port`.
    pub fn set_port_persister(&self, f: Box<dyn Fn(&str, u16) + Send + Sync>) {
        *self.on_port_assigned.lock().unwrap() = Some(f);
    }

    /// Start a WS server for `project_id`, returning the port it listens on.
    /// If a server is already running for the project, returns its existing port.
    pub async fn start_for_project(self: &Arc<Self>, project_id: &str) -> u16 {
        // Already started?
        if let Some(ch) = self.channels.lock().unwrap().get(project_id) {
            return ch.port;
        }

        let port = self.allocate_port();
        let (tx, _rx) = broadcast::channel::<String>(1024);

        // Spawn the TCP/WS accept loop for this project.
        let tx_clone = tx.clone();
        let pid = project_id.to_string();
        let hub = self.clone();
        tokio::spawn(async move {
            hub.run_accept_loop(port, tx_clone, pid).await;
        });

        // Persist the port (so the GUI can connect).
        if let Some(cb) = self.on_port_assigned.lock().unwrap().as_ref() {
            cb(project_id, port);
        }

        self.channels.lock().unwrap().insert(
            project_id.to_string(),
            ProjectChannel { port, tx },
        );
        port
    }

    /// Publish a message to every subscriber of `project_id`.
    pub fn publish(&self, project_id: &str, msg: &WsMessage) {
        let tx = {
            let guard = self.channels.lock().unwrap();
            guard.get(project_id).map(|ch| ch.tx.clone())
        };
        if let Some(tx) = tx {
            if let Ok(json) = serde_json::to_string(msg) {
                // Ignore send errors when no subscribers are connected.
                let _ = tx.send(json);
            }
        }
    }

    /// Look up the WS port for a project (0 = not started).
    pub fn port_for(&self, project_id: &str) -> u16 {
        self.channels
            .lock()
            .unwrap()
            .get(project_id)
            .map(|ch| ch.port)
            .unwrap_or(0)
    }

    // ── internals ────────────────────────────────────────────────

    fn allocate_port(&self) -> u16 {
        // Try ports in a project-stable band so they are easy to reason about.
        // Start from 18700 and walk up until a free one is found.
        let base: u16 = 18700;
        for offset in 0..2000u16 {
            let port = base + offset;
            if !self.port_in_use(port) {
                // Probe actual bindability quickly.
                if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
                    return port;
                }
            }
        }
        // Fallback: OS-assigned port.
        if let Ok(l) = std::net::TcpListener::bind(("127.0.0.1", 0)) {
            if let Ok(addr) = l.local_addr() {
                return addr.port();
            }
        }
        0
    }

    fn port_in_use(&self, port: u16) -> bool {
        self.channels
            .lock()
            .unwrap()
            .values()
            .any(|ch| ch.port == port)
    }

    async fn run_accept_loop(self: &Arc<Self>, port: u16, tx: Tx, project_id: String) {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[WS] Failed to bind {} for {}: {}", addr, project_id, e);
                return;
            }
        };
        println!("[WS] Listening on ws://{} for project {}", addr, project_id);

        loop {
            let (stream, _peer) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => continue,
            };
            let rx = tx.subscribe();
            tokio::spawn(Self::handle_connection(stream, rx, project_id.clone()));
        }
    }

    async fn handle_connection(stream: TcpStream, mut rx: broadcast::Receiver<String>, project_id: String) {
        let ws_stream = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                eprintln!("[WS] WS handshake failed for {}: {}", project_id, e);
                return;
            }
        };
        let (mut writer, _reader) = ws_stream.split();

        // Send a hello frame so the client knows it connected.
        // (Broadcast messages delivered after subscribe won't reach a
        //  just-connected client, so greet it directly.)
        let hello = serde_json::json!({
            "type": "hello",
            "payload": { "project_id": project_id, "server": "crawlflow-service" }
        });
        let _ = writer.send(Message::Text(hello.to_string().into())).await;

        // Forward every broadcast message to this client. A closed/disconnected
        // client surfaces as a write error on the next frame, which ends the task.
        loop {
            match rx.recv().await {
                Ok(text) => {
                    if writer.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}

/// Convenience free functions used by `logs.rs` / `python_plugins.rs` to push
/// events without holding a reference to the hub (a process-global hub is set
/// at service startup).
static GLOBAL_HUB: std::sync::OnceLock<Arc<WsHub>> = std::sync::OnceLock::new();

pub fn set_global_hub(hub: Arc<WsHub>) {
    let _ = GLOBAL_HUB.set(hub);
}

pub fn global_hub() -> Option<Arc<WsHub>> {
    GLOBAL_HUB.get().cloned()
}

/// Publish a per-item event (e.g. a product URL was collected) in realtime.
pub fn publish_item(project_id: &str, payload: serde_json::Value) {
    if let Some(hub) = global_hub() {
        hub.publish(project_id, &WsMessage::item(payload));
    }
}

/// Publish a log entry in realtime.
pub fn publish_log(project_id: &str, payload: serde_json::Value) {
    if let Some(hub) = global_hub() {
        hub.publish(project_id, &WsMessage::log(payload));
    }
}

/// Publish a progress snapshot in realtime.
pub fn publish_progress(project_id: &str, payload: serde_json::Value) {
    if let Some(hub) = global_hub() {
        hub.publish(project_id, &WsMessage::progress(payload));
    }
}
