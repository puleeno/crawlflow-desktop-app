use std::sync::{Arc, OnceLock, Mutex};
use std::time::Duration;

/// Network settings stored in `app_settings` (key prefix: `network_`).
#[derive(Debug, Clone)]
pub struct NetworkSettings {
    /// Keep TCP connections alive between requests (default: true).
    pub keep_alive: bool,
    /// Max connections per host in the pool (default: 8).
    pub pool_size: u32,
    /// Per-request timeout in seconds (default: 30).
    pub timeout_secs: u64,
    /// Number of automatic retries on transient errors (default: 3).
    pub retry_count: u32,
    /// Base delay between retries in milliseconds (default: 1000).
    pub retry_delay_ms: u64,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            keep_alive: true,
            pool_size: 8,
            timeout_secs: 30,
            retry_count: 3,
            retry_delay_ms: 1000,
        }
    }
}

impl NetworkSettings {
    /// Load from `app_settings` table. Missing keys fall back to defaults.
    pub fn load_from_db(db_path: &std::path::Path) -> Self {
        let get = |key: &str| -> Option<String> {
            let conn = rusqlite::Connection::open(db_path).ok()?;
            conn.query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .ok()
        };

        Self {
            keep_alive: get("network_keep_alive")
                .map(|v| v != "false")
                .unwrap_or(true),
            pool_size: get("network_pool_size")
                .and_then(|v| v.parse().ok())
                .unwrap_or(8),
            timeout_secs: get("network_timeout_secs")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            retry_count: get("network_retry_count")
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
            retry_delay_ms: get("network_retry_delay_ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000),
        }
    }
}

struct ClientEntry {
    client: reqwest::Client,
    settings_fingerprint: String,
}

/// Global shared HTTP client pool. Thread-safe, lazily initialized.
static SHARED_CLIENT: OnceLock<Arc<Mutex<ClientEntry>>> = OnceLock::new();

fn fingerprint(s: &NetworkSettings) -> String {
    format!(
        "{}|{}|{}",
        s.keep_alive, s.pool_size, s.timeout_secs
    )
}

/// Get (or rebuild) the shared reqwest client. If settings changed since last
/// call, the client is rebuilt transparently.
pub fn get_shared_client(settings: &NetworkSettings) -> Result<reqwest::Client, String> {
    let entry_ref = SHARED_CLIENT.get_or_init(|| Arc::new(Mutex::new(ClientEntry {
        client: build_client(settings).unwrap_or_else(|_| build_client(&NetworkSettings::default()).unwrap()),
        settings_fingerprint: fingerprint(settings),
    })));

    let mut guard = entry_ref.lock().unwrap();
    let fp = fingerprint(settings);
    if guard.settings_fingerprint != fp {
        guard.client = build_client(settings)?;
        guard.settings_fingerprint = fp;
    }
    Ok(guard.client.clone())
}

fn build_client(s: &NetworkSettings) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .danger_accept_invalid_certs(false)
        .timeout(Duration::from_secs(s.timeout_secs))
        .pool_max_idle_per_host(s.pool_size as usize)
        .connect_timeout(Duration::from_secs(10));

    if s.keep_alive {
        builder = builder
            .tcp_keepalive(Some(Duration::from_secs(60)))
            .tcp_nodelay(true);
    } else {
        builder = builder
            .tcp_keepalive(None);
    }

    builder
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

/// Retry wrapper: tries up to `retry_count` times with exponential backoff.
pub async fn fetch_with_retry(
    url: &str,
    settings: &NetworkSettings,
    build_request: impl Fn(&reqwest::Client) -> reqwest::RequestBuilder,
) -> Result<reqwest::Response, String> {
    let client = get_shared_client(settings)?;
    let mut last_err = String::new();

    for attempt in 0..=settings.retry_count {
        let req = build_request(&client);
        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if status == 429 || status >= 500 {
                    last_err = format!("HTTP {}", status);
                    if attempt < settings.retry_count {
                        let delay = settings.retry_delay_ms * (1u64 << attempt.min(4));
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                        continue;
                    }
                    return Ok(resp);
                }
                return Ok(resp);
            }
            Err(e) => {
                let is_transient = e.is_timeout()
                    || e.is_connect()
                    || e.to_string().contains("connection reset")
                    || e.to_string().contains("broken pipe");
                last_err = e.to_string();
                if is_transient && attempt < settings.retry_count {
                    let delay = settings.retry_delay_ms * (1u64 << attempt.min(4));
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    continue;
                }
                return Err(last_err);
            }
        }
    }
    Err(last_err)
}
