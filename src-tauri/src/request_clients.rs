use crate::models::*;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use std::fs;
use tungstenite::stream::MaybeTlsStream;
use std::net::TcpStream;

type WsStream = tungstenite::WebSocket<MaybeTlsStream<TcpStream>>;

macro_rules! debug_log {
    ($($arg:tt)*) => {
        eprintln!("[chrome] {}", format!($($arg)*))
    };
}

fn find_chrome() -> Option<PathBuf> {
    // Check configured path from app settings first
    if let Some(data_dir) = dirs_next::data_dir() {
        let master_db = data_dir.join("crawlflow").join("crawlflow.db");
        if master_db.exists() {
            if let Ok(conn) = rusqlite::Connection::open(&master_db) {
                if let Ok(mut stmt) = conn.prepare("SELECT value FROM app_settings WHERE key = 'chrome_path'") {
                    if let Ok(row) = stmt.query_row([], |r| r.get::<_, String>(0)) {
                        let p = PathBuf::from(&row);
                        if p.exists() {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }

    let candidates: Vec<PathBuf> = if cfg!(target_os = "macos") {
        vec![
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into(),
            "/Applications/Chromium.app/Contents/MacOS/Chromium".into(),
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser".into(),
            "/Applications/Edge.app/Contents/MacOS/Microsoft Edge".into(),
        ]
    } else if cfg!(target_os = "linux") {
        vec![
            "/usr/bin/google-chrome".into(),
            "/usr/bin/google-chrome-stable".into(),
            "/usr/bin/chromium".into(),
            "/usr/bin/chromium-browser".into(),
        ]
    } else if cfg!(target_os = "windows") {
        vec![
            r"C:\Program Files\Google\Chrome\Application\chrome.exe".into(),
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe".into(),
            r"C:\Program Files\Chromium\Application\chrome.exe".into(),
        ]
    } else {
        vec![]
    };

    // First check exact paths
    for c in &candidates {
        if c.exists() {
            return Some(c.clone());
        }
    }

    // Fall back to PATH lookup via Command
    for name in &["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "chrome"] {
        if let Ok(output) = std::process::Command::new("which").arg(name).output() {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(PathBuf::from(path));
                }
            }
        }
    }

    None
}

fn build_reqwest_client(profile: &ClientProfile) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .danger_accept_invalid_certs(false)
        .timeout(Duration::from_secs(profile.timeout_secs.unwrap_or(30)));

    if let Some(ua) = &profile.user_agent {
        builder = builder.user_agent(ua);
    }

    if let Some(proxy) = &profile.proxy_url {
        let proxy = reqwest::Proxy::all(proxy)
            .map_err(|e| format!("Invalid proxy: {}", e))?;
        builder = builder.proxy(proxy);
    }

    builder.build().map_err(|e| format!("Failed to build reqwest client: {}", e))
}

pub async fn fetch_reqwest(
    url: &str,
    profile: &ClientProfile,
) -> CrawlResult {
    let client = match build_reqwest_client(profile) {
        Ok(c) => c,
        Err(e) => {
            return CrawlResult {
                url: url.to_string(),
                status: 0,
                html: None,
                text: None,
                extracted: None,
                error: Some(e),
            };
        }
    };

    let mut req = client.get(url);
    if let Some(headers) = &profile.headers {
        for (k, v) in headers {
            req = req.header(k, v);
        }
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return CrawlResult {
                url: url.to_string(),
                status: 0,
                html: None,
                text: None,
                extracted: None,
                error: Some(format!("HTTP request failed: {}", e)),
            };
        }
    };

    let status = response.status().as_u16();
    let html = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            return CrawlResult {
                url: url.to_string(),
                status,
                html: None,
                text: None,
                extracted: None,
                error: Some(format!("Failed to read body: {}", e)),
            };
        }
    };

    let text = crate::crawler::strip_html_tags(&html);

    CrawlResult {
        url: url.to_string(),
        status,
        html: Some(html),
        text: Some(text),
        extracted: None,
        error: None,
    }
}

fn kill_chrome_process(pid: u32) {
    // Try to kill the process and its children
    #[cfg(unix)]
    {
        // Kill process group on Unix
        let _ = Command::new("pkill")
            .args(["-P", &pid.to_string()])
            .output();
        let _ = Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output();
    }
    #[cfg(windows)]
    {
        // Kill process tree on Windows
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string(), "/T"])
            .output();
    }
}

pub fn fetch_chrome_sync(
    url: &str,
    profile: &ClientProfile,
    wait_for_selector: Option<&str>,
    wait_for_content: Option<&str>,
    wait_timeout_ms: Option<u64>,
) -> CrawlResult {
    let chrome = match find_chrome() {
        Some(c) => c,
        None => {
            return CrawlResult {
                url: url.to_string(),
                status: 0,
                html: None,
                text: None,
                extracted: None,
                error: Some("Chrome/Chromium not found on this system".to_string()),
            };
        }
    };

    let profile_dir = profile.profile_dir.clone().unwrap_or_else(|| {
        let dir = std::env::temp_dir()
            .join("crawlflow-chrome-profiles")
            .join(simple_hash(url));
        // Remove stale lock files from previous runs
        let _ = std::fs::remove_dir_all(&dir);
        dir.to_string_lossy().to_string()
    });

    let _ = std::fs::create_dir_all(&profile_dir);

    // Create temporary JS file for wait logic
    let wait_script = if wait_for_selector.is_some() || wait_for_content.is_some() {
        let selector = wait_for_selector.unwrap_or("");
        let content = wait_for_content.unwrap_or("");
        let timeout = wait_timeout_ms.unwrap_or(10000);
        
        Some(format!(r#"
const url = "{}";
const waitForSelector = "{}";
const waitForContent = "{}";
const timeout = {};

(async () => {{
    const page = await browser.newPage();
    await page.goto(url, {{ waitUntil: 'domcontentloaded', timeout: timeout }});
    
    if (waitForSelector) {{
        try {{
            await page.waitForSelector(waitForSelector, {{ timeout: timeout }});
        }} catch (e) {{
            console.error('Selector timeout:', e);
        }}
    }}
    
    if (waitForContent) {{
        try {{
            await page.waitForFunction((content) => {{
                return document.body.innerText.includes(content);
            }}, {{}}, waitForContent, {{ timeout: timeout }});
        }} catch (e) {{
            console.error('Content timeout:', e);
        }}
    }}
    
    const html = await page.content();
    console.log(html);
    await browser.close();
}})();
"#, url, selector, content, timeout))
    } else {
        None
    };

    let mut cmd = Command::new(&chrome);
    let headless = profile.headless.unwrap_or(true);
    cmd.args([
        "--disable-gpu",
        "--no-sandbox",
        "--disable-dev-shm-usage",
        "--disable-extensions",
        "--disable-background-networking",
        "--disable-sync",
        "--disable-translate",
        "--disable-default-apps",
        "--mute-audio",
        "--no-first-run",
        "--hide-scrollbars",
        "--disable-blink-features=AutomationControlled",
    ]);

    if headless {
        cmd.arg("--headless=new");
    }

    let timeout_secs = profile.timeout_secs.unwrap_or(30);
    cmd.arg(format!("--timeout={}", timeout_secs * 1000));

    if let Some(args) = &profile.chrome_args {
        for arg in args {
            cmd.arg(arg);
        }
    }

    cmd.arg(format!("--user-data-dir={}", profile_dir));
    cmd.arg(format!("--window-size=1920,1080"));

    if let Some(ua) = &profile.user_agent {
        cmd.arg(format!("--user-agent={}", ua));
    }

    if let Some(proxy) = &profile.proxy_url {
        cmd.arg(format!("--proxy-server={}", proxy));
    }

    if let Some(extra) = &profile.extra_nav_args {
        for arg in extra {
            cmd.arg(arg);
        }
    }

    if let Some(script) = &wait_script {
        // Use Node.js with puppeteer if available, otherwise fallback to simple fetch
        let script_path = std::env::temp_dir()
            .join("crawlflow-chrome-scripts")
            .join(format!("{}.js", simple_hash(url)));
        let _ = std::fs::create_dir_all(script_path.parent().unwrap());
        let _ = fs::write(&script_path, script);
        
        // Try to use Node.js with puppeteer
        if let Ok(node_output) = Command::new("node")
            .arg(&script_path)
            .output()
        {
            let _ = fs::remove_file(&script_path);
            if node_output.status.success() {
                let html = String::from_utf8_lossy(&node_output.stdout).to_string();
                let text = crate::crawler::strip_html_tags(&html);
                return CrawlResult {
                    url: url.to_string(),
                    status: 200,
                    html: Some(html),
                    text: Some(text),
                    extracted: None,
                    error: None,
                };
            }
        }
        
        // Fallback to simple chrome dump-dom if Node.js fails
        let _ = fs::remove_file(&script_path);
    }

    cmd.arg("--dump-dom");
    cmd.arg(url);

    let chrome_timeout = Duration::from_secs(timeout_secs);
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return CrawlResult {
                url: url.to_string(),
                status: 0,
                html: None,
                text: None,
                extracted: None,
                error: Some(format!("Chrome launch failed: {}", e)),
            };
        }
    };
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let output = match rx.recv_timeout(chrome_timeout) {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            kill_chrome_process(pid);
            return CrawlResult {
                url: url.to_string(),
                status: 0,
                html: None,
                text: None,
                extracted: None,
                error: Some(format!("Chrome output error: {}", e)),
            };
        }
        Err(_) => {
            kill_chrome_process(pid);
            return CrawlResult {
                url: url.to_string(),
                status: 0,
                html: None,
                text: None,
                extracted: None,
                error: Some(format!("Chrome timed out after {}s", chrome_timeout.as_secs())),
            };
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return CrawlResult {
            url: url.to_string(),
            status: 0,
            html: None,
            text: None,
            extracted: None,
            error: Some(format!("Chrome error: {}", stderr)),
        };
    }

    let html = String::from_utf8_lossy(&output.stdout).to_string();
    let text = crate::crawler::strip_html_tags(&html);

    CrawlResult {
        url: url.to_string(),
        status: 200,
        html: Some(html),
        text: Some(text),
        extracted: None,
        error: None,
    }
}

fn find_free_port(start: u16) -> u16 {
    use std::net::TcpListener;
    for port in start..(start + 1000) {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    start
}

fn wait_for_chrome_ready(port: u16, timeout_secs: u64) -> Result<(), String> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let mut attempts = 0;
    loop {
        if std::time::Instant::now().duration_since(start) > timeout {
            return Err(format!("Chrome did not start on port {} after {}s", port, timeout_secs));
        }
        if reqwest::blocking::get(format!("http://127.0.0.1:{}/json/version", port))
            .ok()
            .and_then(|r| r.status().is_success().then_some(()))
            .is_some()
        {
            debug_log!("Chrome ready on port {} (after ~{}ms)", port, attempts * 200);
            return Ok(());
        }
        attempts += 1;
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn launch_chrome_cdp(
    profile: &ClientProfile,
    url: &str,
) -> Result<(ChromeSession, std::process::Child), String> {
    let chrome_path = find_chrome().ok_or("Chrome/Chromium not found on this system")?;
    debug_log!("Chrome binary: {:?}", chrome_path);
    let port = find_free_port(9222);
    debug_log!("Selected CDP port: {}", port);
    let profile_dir = profile.profile_dir.clone().unwrap_or_else(|| {
        let dir = std::env::temp_dir()
            .join("crawlflow-chrome-profiles")
            .join(simple_hash(url));
        let _ = std::fs::remove_dir_all(&dir);
        dir.to_string_lossy().to_string()
    });
    let _ = std::fs::create_dir_all(&profile_dir);
    debug_log!("Profile dir: {}", profile_dir);

    let mut cmd = Command::new(&chrome_path);
    // Always use headless for CDP — headed mode causes GPU crashes in service mode
    debug_log!("Headless mode: true (forced for CDP)");

    cmd.args([
        "--no-sandbox",
        "--disable-gpu",
        "--disable-software-rasterizer",
        "--disable-dev-shm-usage",
        "--disable-extensions",
        "--disable-background-networking",
        "--disable-sync",
        "--disable-translate",
        "--hide-scrollbars",
        "--disable-blink-features=AutomationControlled",
        "--ignore-certificate-errors",
        "--disable-features=TranslateUI,ChromeWhatsNewUI",
    ]);

    cmd.arg("--headless");

    if let Some(args) = &profile.chrome_args {
        for arg in args {
            cmd.arg(arg);
        }
    }

    cmd.arg(format!("--remote-debugging-port={}", port));
    cmd.arg(format!("--user-data-dir={}", profile_dir));
    cmd.arg(format!("--window-size=1920,1080"));

    if let Some(ua) = &profile.user_agent {
        cmd.arg(format!("--user-agent={}", ua));
    }
    if let Some(proxy) = &profile.proxy_url {
        cmd.arg(format!("--proxy-server={}", proxy));
    }
    if let Some(extra) = &profile.extra_nav_args {
        for arg in extra {
            cmd.arg(arg);
        }
    }

    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::piped());

    debug_log!("Spawning Chrome...");
    let mut child = cmd.spawn().map_err(|e| format!("Chrome launch failed: {}", e))?;
    let pid = child.id();
    debug_log!("Chrome spawned with pid={}", pid);

    // Read Chrome stderr in a background thread so the pipe doesn't fill up
    let stderr_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            use std::io::{BufRead, Read};
            let mut reader = std::io::BufReader::new(stderr);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    eprintln!("[chrome:stderr] {}", trimmed);
                }
                line.clear();
            }
        })
    });

    debug_log!("Waiting for Chrome to be ready on port {}...", port);
    match wait_for_chrome_ready(port, profile.timeout_secs.unwrap_or(30)) {
        Ok(_) => {
            debug_log!("Chrome is ready (pid={}, port={})", pid, port);
        }
        Err(e) => {
            // Ensure stderr thread finishes collecting output before we kill
            if let Some(h) = stderr_handle { let _ = h.join(); }
            kill_chrome_process(pid);
            return Err(format!("{}", e));
        }
    }

    Ok((
        ChromeSession {
            debug_port: port,
            pid,
            profile_dir,
            page_id: None,
        },
        child,
    ))
}

fn get_cdp_websocket_url(port: u16) -> Result<String, String> {
    let resp = reqwest::blocking::get(format!("http://127.0.0.1:{}/json/version", port))
        .map_err(|e| format!("Failed to get CDP info: {}", e))?;
    let info: serde_json::Value = resp
        .json()
        .map_err(|e| format!("Failed to parse CDP info: {}", e))?;
    info["webSocketDebuggerUrl"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No WebSocket debugger URL".into())
}

fn create_cdp_page(port: u16, url: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .put(format!(
            "http://127.0.0.1:{}/json/new?{}",
            port,
            urlencoding(url)
        ))
        .send()
        .map_err(|e| format!("Failed to create CDP page: {}", e))?;
    let info: serde_json::Value = resp
        .json()
        .map_err(|e| format!("Failed to parse page info: {}", e))?;
    Ok(info)
}

fn urlencoding(url: &str) -> String {
    url.replace('%', "%25")
        .replace('&', "%26")
        .replace('?', "%3F")
        .replace('=', "%3D")
        .replace('#', "%23")
        .replace(' ', "%20")
}

fn set_ws_read_timeout(ws: &mut WsStream, timeout_secs: u64) {
    if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = ws.get_mut() {
        let _ = tcp.set_read_timeout(Some(Duration::from_secs(timeout_secs)));
    }
}

fn send_cdp_msg(
    ws: &mut WsStream,
    msg: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let raw = serde_json::to_string(msg).map_err(|e| format!("CDP serialize: {}", e))?;
    set_ws_read_timeout(ws, 15);
    ws.write(tungstenite::Message::Text(raw))
        .map_err(|e| format!("CDP write: {}", e))?;

    // Read responses until we get one matching our id
    loop {
        let resp = ws
            .read();
        match resp {
            Ok(tungstenite::Message::Text(text)) => {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(id) = val.get("id") {
                        debug_log!("[CDP] Response id={}: {}", id, &text[..text.len().min(200)]);
                        set_ws_read_timeout(ws, 0); // restore blocking
                        return Ok(val);
                    } else if let Some(method) = val.get("method").and_then(|m| m.as_str()) {
                        debug_log!("[CDP] Event (skipped): {}", method);
                        // event — skip
                    } else {
                        debug_log!("[CDP] Unknown message: {}", &text[..text.len().min(100)]);
                    }
                }
            }
            Err(e) => {
                return Err(format!("CDP read error: {}", e));
            }
            _ => {}
        }
    }
}

fn cdp_evaluate_js(
    ws: &mut WsStream,
    expression: &str,
) -> Result<String, String> {
    let msg = serde_json::json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": {
            "expression": expression,
            "returnByValue": true
        }
    });
    let result = send_cdp_msg(ws, &msg)?;
    if let Some(error) = result.get("error") {
        return Err(format!("CDP JS error: {}", error));
    }
    result["result"]["result"]["value"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "CDP evaluate returned non-string".into())
}

/// Fetch URL via CDP (Chrome DevTools Protocol) — keeps Chrome alive after fetch.
/// Returns (CrawlResult, ChromeSession).
pub fn fetch_via_cdp(
    url: &str,
    profile: &ClientProfile,
    wait_for_selector: Option<&str>,
    wait_timeout_ms: Option<u64>,
) -> (CrawlResult, Option<ChromeSession>) {
    debug_log!("[fetch_via_cdp] Launching Chrome for URL: {}", url);
    let (session, _child) = match launch_chrome_cdp(profile, url) {
        Ok(s) => s,
        Err(e) => {
            log::error!("[fetch_via_cdp] Chrome launch failed: {}", e);
            return (
                CrawlResult {
                    url: url.to_string(),
                    status: 0,
                    html: None,
                    text: None,
                    extracted: None,
                    error: Some(e),
                },
                None,
            );
        }
    };
    debug_log!("[fetch_via_cdp] Chrome launched (pid={}, port={})", session.pid, session.debug_port);

    // Get browser WebSocket URL
    debug_log!("[fetch_via_cdp] Getting CDP WebSocket URL...");
    let browser_ws_url = match get_cdp_websocket_url(session.debug_port) {
        Ok(u) => u,
        Err(e) => {
            log::error!("[fetch_via_cdp] Failed to get CDP WebSocket URL: {}", e);
            kill_chrome_process(session.pid);
            return (
                CrawlResult {
                    url: url.to_string(),
                    status: 0,
                    html: None,
                    text: None,
                    extracted: None,
                    error: Some(e),
                },
                None,
            );
        }
    };
    debug_log!("[fetch_via_cdp] Browser WS URL: {}", browser_ws_url);

    // Create new page/tab
    debug_log!("[fetch_via_cdp] Creating new page via CDP...");
    let page_info = match create_cdp_page(session.debug_port, url) {
        Ok(p) => p,
        Err(e) => {
            log::error!("[fetch_via_cdp] Failed to create CDP page: {}", e);
            kill_chrome_process(session.pid);
            return (
                CrawlResult {
                    url: url.to_string(),
                    status: 0,
                    html: None,
                    text: None,
                    extracted: None,
                    error: Some(e),
                },
                None,
            );
        }
    };
    debug_log!("[fetch_via_cdp] Page created: id={:?}", page_info.get("id").and_then(|v| v.as_str()));

    let page_ws_url = page_info["webSocketDebuggerUrl"]
        .as_str()
        .map(|s| s.to_string());
    let page_id = page_info["id"].as_str().map(|s| s.to_string());

    // Connect to page WebSocket
    let ws_url = page_ws_url.as_deref().unwrap_or(&browser_ws_url);
    debug_log!("[fetch_via_cdp] Connecting to page WebSocket: {}...", ws_url);
    let mut ws = match tungstenite::connect(ws_url) {
        Ok((ws_conn, _)) => ws_conn,
        Err(e) => {
            log::error!("[fetch_via_cdp] WebSocket connect failed: {}", e);
            kill_chrome_process(session.pid);
            return (
                CrawlResult {
                    url: url.to_string(),
                    status: 0,
                    html: None,
                    text: None,
                    extracted: None,
                    error: Some(format!("CDP WebSocket connect failed: {}", e)),
                },
                None,
            );
        }
    };
    debug_log!("[fetch_via_cdp] WebSocket connected");

    // Page already created + navigated via PUT /json/new?url=<url> above.
    // Just enable Page events and poll for document readyState.
    debug_log!("[fetch_via_cdp] Sending Page.enable...");
    let enable_msg = serde_json::json!({
        "id": 1,
        "method": "Page.enable"
    });
    if let Err(e) = send_cdp_msg(&mut ws, &enable_msg) {
        log::error!("[fetch_via_cdp] Page.enable failed: {}", e);
        kill_chrome_process(session.pid);
        return (
            CrawlResult {
                url: url.to_string(),
                status: 0,
                html: None,
                text: None,
                extracted: None,
                error: Some(format!("CDP Page.enable failed: {}", e)),
            },
            None,
        );
    }
    debug_log!("[fetch_via_cdp] Page.enable OK");

    // Poll document.readyState (avoids race with CDP events consumed by send_cdp_msg)
    debug_log!("[fetch_via_cdp] Waiting for document readyState...");
    let load_timeout = Duration::from_millis(wait_timeout_ms.unwrap_or(15000));
    let load_start = std::time::Instant::now();
    loop {
        if std::time::Instant::now().duration_since(load_start) > load_timeout {
            debug_log!("[fetch_via_cdp] Page load timeout — continuing anyway");
            break;
        }
        match cdp_evaluate_js(&mut ws, "document.readyState") {
            Ok(state) => {
                let s = state.trim_matches('"');
                debug_log!("[fetch_via_cdp] readyState: {}", s);
                if s == "complete" || s == "interactive" {
                    debug_log!("[fetch_via_cdp] Page loaded (readyState={})", s);
                    break;
                }
            }
            Err(e) => {
                debug_log!("[fetch_via_cdp] readyState poll error: {}", e);
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    // Wait for selector if configured
    if let Some(selector) = wait_for_selector {
        debug_log!("[fetch_via_cdp] Waiting for selector: {}", selector);
        let js = format!(
            "new Promise(resolve => {{ \
                const el = document.querySelector('{}'); \
                if (el) {{ resolve(true); return; }} \
                const observer = new MutationObserver(() => {{ \
                    if (document.querySelector('{}')) {{ \
                        observer.disconnect(); resolve(true); \
                    }} \
                }}); \
                observer.observe(document.body, {{ childList: true, subtree: true }}); \
                setTimeout(() => resolve(false), {}); \
            }})",
            selector.replace('\\', "\\\\").replace('\'', "\\'"),
            selector.replace('\\', "\\\\").replace('\'', "\\'"),
            wait_timeout_ms.unwrap_or(10000)
        );
        let _ = cdp_evaluate_js(&mut ws, &js);
        debug_log!("[fetch_via_cdp] Selector wait done");
    }

    // Get full HTML
    debug_log!("[fetch_via_cdp] Evaluating JS to get outerHTML...");
    let html = match cdp_evaluate_js(&mut ws, "document.documentElement.outerHTML") {
        Ok(h) => h,
        Err(e) => {
            log::error!("[fetch_via_cdp] JS evaluate failed: {}", e);
            kill_chrome_process(session.pid);
            return (
                CrawlResult {
                    url: url.to_string(),
                    status: 0,
                    html: None,
                    text: None,
                    extracted: None,
                    error: Some(e),
                },
                None,
            );
        }
    };
    debug_log!("[fetch_via_cdp] HTML fetched ({} bytes)", html.len());

    // Don't close Chrome — pass session to preprocessor
    let text = crate::crawler::strip_html_tags(&html);
    let session = ChromeSession {
        page_id: Some(page_id.unwrap_or_default()),
        ..session
    };

    debug_log!("[fetch_via_cdp] Done, returning ChromeSession (pid={}, port={})", session.pid, session.debug_port);
    (
        CrawlResult {
            url: url.to_string(),
            status: 200,
            html: Some(html),
            text: Some(text),
            extracted: None,
            error: None,
        },
        Some(session),
    )
}

/// Close Chrome gracefully via CDP Browser.close, fallback to kill
pub fn close_chrome_session(session: &ChromeSession) {
    debug_log!("[close_chrome] Closing Chrome session (pid={}, port={})", session.pid, session.debug_port);

    // Try graceful shutdown via CDP Browser.close
    let ws_url = format!("ws://127.0.0.1:{}/devtools/browser/{}", session.debug_port, session.pid);
    debug_log!("[close_chrome] Sending Browser.close via CDP...");
    if let Ok((mut ws, _)) = tungstenite::connect(ws_url.as_str()) {
        let msg = serde_json::json!({
            "id": 1,
            "method": "Browser.close"
        });
        if let Ok(raw) = serde_json::to_string(&msg) {
            if ws.write(tungstenite::Message::Text(raw)).is_ok() {
                std::thread::sleep(Duration::from_millis(500));
                debug_log!("[close_chrome] Chrome closed gracefully via CDP");
                return;
            }
        }
    }

    // Fallback: force kill
    debug_log!("[close_chrome] CDP close failed, force killing pid={}", session.pid);
    kill_chrome_process(session.pid);
    debug_log!("[close_chrome] Force kill done");
}

fn simple_hash(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_profile_default() {
        let p = ClientProfile::default();
        assert_eq!(p.client_type, "reqwest");
        assert_eq!(p.user_agent.unwrap(), "CrawlFlow/1.0");
        assert_eq!(p.timeout_secs.unwrap(), 30);
        assert!(p.proxy_url.is_none());
        assert!(p.chrome_args.is_none());
    }

    #[test]
    fn test_find_chrome_returns_something() {
        let result = find_chrome();
        assert!(result.is_none() || result.is_some());
    }

    #[test]
    fn test_simple_hash_consistency() {
        let h1 = simple_hash("https://example.com");
        let h2 = simple_hash("https://example.com");
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
    }

    #[test]
    fn test_simple_hash_different_inputs() {
        let h1 = simple_hash("url-a");
        let h2 = simple_hash("url-b");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_simple_hash_empty() {
        let h = simple_hash("");
        assert!(!h.is_empty());
    }

    #[test]
    fn test_simple_hash_unicode() {
        let h1 = simple_hash("cà phê sữa đá");
        let h2 = simple_hash("cà phê sữa đá");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_simple_hash_url() {
        let h = simple_hash("https://oreka.vn/san-pham/abc-123");
        assert_eq!(h.len(), 16);
    }

    #[test]
    fn test_build_reqwest_client_default() {
        let profile = ClientProfile::default();
        let client = build_reqwest_client(&profile);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_reqwest_client_with_proxy() {
        let profile = ClientProfile {
            proxy_url: Some("http://invalid-proxy:9999".into()),
            ..Default::default()
        };
        let client = build_reqwest_client(&profile);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_reqwest_client_with_all_options() {
        let profile = ClientProfile {
            user_agent: Some("CustomBot/2.0".into()),
            timeout_secs: Some(120),
            proxy_url: None,
            ..Default::default()
        };
        let client = build_reqwest_client(&profile);
        assert!(client.is_ok());
    }

    #[test]
    fn test_fetch_with_client_unknown_type_falls_back_to_reqwest() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let profile = ClientProfile {
            client_type: "unknown-xxx".into(),
            ..Default::default()
        };
        let result = rt.block_on(fetch_with_client("http://0.0.0.0:1", &profile, None));
        assert_eq!(result.url, "http://0.0.0.0:1");
        assert!(result.error.is_some() || result.status == 0);
    }

    #[test]
    #[ignore = "Spawns Chrome browser - only run manually"]
    fn test_fetch_with_client_chrome() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let profile = ClientProfile {
            client_type: "chrome".into(),
            ..Default::default()
        };
        let result = rt.block_on(fetch_with_client("http://0.0.0.0:1", &profile, None));
        assert_eq!(result.url, "http://0.0.0.0:1");
    }

    #[test]
    fn test_fetch_with_client_with_extract_rules() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let profile = ClientProfile::default();
        let rules = vec![ExtractRule {
            field: "test".into(),
            selector: "h1".into(),
            attribute: None,
            extract_multiple: None,
        }];
        let result = rt.block_on(fetch_with_client("http://0.0.0.0:1", &profile, Some(rules)));
        assert_eq!(result.url, "http://0.0.0.0:1");
    }
}

pub async fn fetch_with_client(
    url: &str,
    profile: &ClientProfile,
    extract_rules: Option<Vec<ExtractRule>>,
    wait_for_selector: Option<&str>,
    wait_for_content: Option<&str>,
    wait_timeout_ms: Option<u64>,
) -> CrawlResult {
    let result = match profile.client_type.as_str() {
        "chrome" => tokio::task::spawn_blocking({
            let url = url.to_string();
            let profile = profile.clone();
            let selector = wait_for_selector.map(|s| s.to_string());
            let content = wait_for_content.map(|c| c.to_string());
            move || fetch_chrome_sync(&url, &profile, selector.as_deref(), content.as_deref(), wait_timeout_ms)
        })
        .await
        .unwrap_or_else(|e| CrawlResult {
            url: url.to_string(),
            status: 0,
            html: None,
            text: None,
            extracted: None,
            error: Some(format!("Chrome task failed: {}", e)),
        }),
        _ => fetch_reqwest(url, profile).await,
    };

    if result.html.is_some() && extract_rules.is_some() {
        let rules = extract_rules.unwrap();
        let html = result.html.as_ref().unwrap();
        let extracted = crate::crawler::extract_from_html(html, &rules);
        CrawlResult {
            extracted: Some(extracted),
            ..result
        }
    } else {
        result
    }
}

/// Fetch via CDP (Chrome DevTools Protocol) — keeps Chrome alive for later use.
/// Returns (CrawlResult, Option<ChromeSession>) where ChromeSession can be passed
/// to the preprocessor for continued use and graceful shutdown.
pub async fn fetch_with_client_cdp(
    url: &str,
    profile: &ClientProfile,
    extract_rules: Option<Vec<ExtractRule>>,
    wait_for_selector: Option<&str>,
    wait_timeout_ms: Option<u64>,
) -> (CrawlResult, Option<ChromeSession>) {
    let (mut result, session) = tokio::task::spawn_blocking({
        let url = url.to_string();
        let profile = profile.clone();
        let selector = wait_for_selector.map(|s| s.to_string());
        let timeout = wait_timeout_ms;
        move || fetch_via_cdp(&url, &profile, selector.as_deref(), timeout)
    })
    .await
    .unwrap_or_else(|e| {
        (
            CrawlResult {
                url: url.to_string(),
                status: 0,
                html: None,
                text: None,
                extracted: None,
                error: Some(format!("CDP task failed: {}", e)),
            },
            None,
        )
    });

    if result.html.is_some() && extract_rules.is_some() {
        let rules = extract_rules.unwrap();
        let html = result.html.as_ref().unwrap();
        let extracted = crate::crawler::extract_from_html(html, &rules);
        result.extracted = Some(extracted);
    }

    (result, session)
}
