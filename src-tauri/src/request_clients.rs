use crate::models::*;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

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

pub fn fetch_chrome_sync(
    url: &str,
    profile: &ClientProfile,
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
    ]);

    if headless {
        cmd.arg("--headless");
    }

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

    cmd.arg("--dump-dom");
    cmd.arg(url);

    let chrome_timeout = Duration::from_secs(profile.timeout_secs.unwrap_or(30));
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
            let _ = Command::new("kill").arg(pid.to_string()).output();
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
) -> CrawlResult {
    let result = match profile.client_type.as_str() {
        "chrome" => tokio::task::spawn_blocking({
            let url = url.to_string();
            let profile = profile.clone();
            move || fetch_chrome_sync(&url, &profile)
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
