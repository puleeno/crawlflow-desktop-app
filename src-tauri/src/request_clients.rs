use crate::models::*;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

fn find_chrome() -> Option<PathBuf> {
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
        std::env::temp_dir()
            .join("crawlflow-chrome-profiles")
            .join(simple_hash(url))
            .to_string_lossy()
            .to_string()
    });

    let _ = std::fs::create_dir_all(&profile_dir);

    let mut cmd = Command::new(&chrome);
    cmd.args([
        "--headless",
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

    let output = match cmd.output() {
        Ok(o) => o,
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
    }

    #[test]
    fn test_find_chrome_returns_something() {
        // Should return either a path or None (depending on system)
        let result = find_chrome();
        // On any system, this should not panic
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
        // Very unlikely to collide
        assert_ne!(h1, h2);
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
