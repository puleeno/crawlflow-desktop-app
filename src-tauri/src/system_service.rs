use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SERVICE_IDENTIFIER: &str = "com.crawlflow.desktop-service";

fn app_exe_path() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

fn data_dir() -> PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("crawlflow")
}

fn service_log_dir() -> PathBuf {
    data_dir().join("logs")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstallInfo {
    pub installed: bool,
    pub running: bool,
    pub auto_start: bool,
    pub service_path: String,
    pub platform: String,
    pub executable: String,
    pub log_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Platform {
    Macos,
    Linux,
    Windows,
    Unknown,
}

impl Platform {
    pub fn detect() -> Self {
        if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Unknown
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Macos => "macos",
            Platform::Linux => "linux",
            Platform::Windows => "windows",
            Platform::Unknown => "unknown",
        }
    }
}

pub struct SystemServiceManager;

impl SystemServiceManager {
    fn plist_path() -> PathBuf {
        dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("LaunchAgents")
            .join(format!("{}.plist", SERVICE_IDENTIFIER))
    }

    fn systemd_path() -> PathBuf {
        dirs_next::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".config")
            .join("systemd")
            .join("user")
            .join(format!("{}.service", SERVICE_IDENTIFIER))
    }

    fn generate_plist(exe: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{identifier}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>--service</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{log}/service-stdout.log</string>
    <key>StandardErrorPath</key>
    <string>{log}/service-stderr.log</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>/usr/local/bin:/usr/bin:/bin:/opt/homebrew/bin</string>
    </dict>
    <key>WorkingDirectory</key>
    <string>{data}</string>
</dict>
</plist>
"#,
            identifier = SERVICE_IDENTIFIER,
            exe = exe,
            log = service_log_dir().to_string_lossy(),
            data = data_dir().to_string_lossy(),
        )
    }

    fn generate_systemd(exe: &str) -> String {
        format!(
            r#"[Unit]
Description=CrawlFlow Background Service
After=network.target

[Service]
Type=simple
ExecStart={exe} --service
Restart=on-failure
RestartSec=5
StandardOutput=append:{log}/service-stdout.log
StandardError=append:{log}/service-stderr.log
WorkingDirectory={data}
Environment=PATH=/usr/local/bin:/usr/bin:/bin

[Install]
WantedBy=default.target
"#,
            exe = exe,
            log = service_log_dir().to_string_lossy(),
            data = data_dir().to_string_lossy(),
        )
    }

    pub fn get_info() -> ServiceInstallInfo {
        let platform = Platform::detect();
        let exe = app_exe_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let installed = Self::is_installed();
        let running = if installed {
            Self::is_running()
        } else {
            false
        };

        let service_path = match platform {
            Platform::Macos => Self::plist_path().to_string_lossy().to_string(),
            Platform::Linux => Self::systemd_path().to_string_lossy().to_string(),
            _ => String::new(),
        };

        ServiceInstallInfo {
            installed,
            running,
            auto_start: installed && running,
            service_path,
            platform: platform.as_str().to_string(),
            executable: exe,
            log_dir: service_log_dir().to_string_lossy().to_string(),
        }
    }

    pub fn is_installed() -> bool {
        match Platform::detect() {
            Platform::Macos => Self::plist_path().exists(),
            Platform::Linux => Self::systemd_path().exists(),
            _ => false,
        }
    }

    pub fn is_running() -> bool {
        match Platform::detect() {
            Platform::Macos => {
                let output = std::process::Command::new("launchctl")
                    .args(["list", SERVICE_IDENTIFIER])
                    .output();
                match output {
                    Ok(out) => out.status.success(),
                    Err(_) => false,
                }
            }
            Platform::Linux => {
                let output = std::process::Command::new("systemctl")
                    .args(["--user", "is-active", SERVICE_IDENTIFIER])
                    .output();
                match output {
                    Ok(out) => {
                        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        s == "active"
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        }
    }

    pub fn install() -> Result<String, String> {
        let exe = app_exe_path()
            .ok_or_else(|| "Cannot determine executable path".to_string())?;
        let exe_str = exe.to_string_lossy().to_string();

        std::fs::create_dir_all(service_log_dir()).map_err(|e| e.to_string())?;

        match Platform::detect() {
            Platform::Macos => {
                let plist_path = Self::plist_path();
                if let Some(parent) = plist_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let plist = Self::generate_plist(&exe_str);
                std::fs::write(&plist_path, plist).map_err(|e| e.to_string())?;

                let output = std::process::Command::new("launchctl")
                    .args(["load", &plist_path.to_string_lossy()])
                    .output()
                    .map_err(|e| format!("Failed to load service: {}", e))?;
                if !output.status.success() {
                    let err = String::from_utf8_lossy(&output.stderr);
                    return Err(format!("launchctl load failed: {}", err));
                }
                Ok(format!("Service installed at {:?}", plist_path))
            }
            Platform::Linux => {
                let unit_path = Self::systemd_path();
                if let Some(parent) = unit_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let unit = Self::generate_systemd(&exe_str);
                std::fs::write(&unit_path, unit).map_err(|e| e.to_string())?;

                let output = std::process::Command::new("systemctl")
                    .args(["--user", "daemon-reload"])
                    .output()
                    .map_err(|e| format!("systemctl daemon-reload failed: {}", e))?;
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "enable", SERVICE_IDENTIFIER])
                    .output();
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "start", SERVICE_IDENTIFIER])
                    .output();

                if !output.status.success() {
                    let err = String::from_utf8_lossy(&output.stderr);
                    return Err(format!("systemctl reload failed: {}", err));
                }
                Ok(format!("Service installed at {:?}", unit_path))
            }
            Platform::Windows => {
                let _ = std::process::Command::new("schtasks")
                    .args([
                        "/CREATE",
                        "/SC",
                        "ONLOGON",
                        "/TN",
                        "CrawlFlowService",
                        "/TR",
                        &exe_str,
                        "/F",
                    ])
                    .output();
                Ok("Service installed (Windows Task Scheduler)".to_string())
            }
            Platform::Unknown => Err("Unsupported platform for service installation".to_string()),
        }
    }

    pub fn uninstall() -> Result<String, String> {
        match Platform::detect() {
            Platform::Macos => {
                let plist_path = Self::plist_path();
                if plist_path.exists() {
                    let _ = std::process::Command::new("launchctl")
                        .args(["unload", &plist_path.to_string_lossy()])
                        .output();
                    std::fs::remove_file(&plist_path).map_err(|e| e.to_string())?;
                }
                Ok("Service uninstalled".to_string())
            }
            Platform::Linux => {
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "stop", SERVICE_IDENTIFIER])
                    .output();
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "disable", SERVICE_IDENTIFIER])
                    .output();
                let unit_path = Self::systemd_path();
                if unit_path.exists() {
                    std::fs::remove_file(&unit_path).map_err(|e| e.to_string())?;
                }
                let _ = std::process::Command::new("systemctl")
                    .args(["--user", "daemon-reload"])
                    .output();
                Ok("Service uninstalled".to_string())
            }
            Platform::Windows => {
                let _ = std::process::Command::new("schtasks")
                    .args(["/DELETE", "/TN", "CrawlFlowService", "/F"])
                    .output();
                Ok("Service uninstalled".to_string())
            }
            Platform::Unknown => Err("Unsupported platform".to_string()),
        }
    }

    pub fn start() -> Result<String, String> {
        match Platform::detect() {
            Platform::Macos => {
                let plist_path = Self::plist_path();
                if !plist_path.exists() {
                    return Err("Service not installed".to_string());
                }
                let output = std::process::Command::new("launchctl")
                    .args(["start", SERVICE_IDENTIFIER])
                    .output()
                    .map_err(|e| format!("launchctl start failed: {}", e))?;
                if output.status.success() {
                    Ok("Service started".to_string())
                } else {
                    let err = String::from_utf8_lossy(&output.stderr);
                    Err(format!("Failed to start: {}", err))
                }
            }
            Platform::Linux => {
                let output = std::process::Command::new("systemctl")
                    .args(["--user", "start", SERVICE_IDENTIFIER])
                    .output()
                    .map_err(|e| e.to_string())?;
                if output.status.success() {
                    Ok("Service started".to_string())
                } else {
                    Err(String::from_utf8_lossy(&output.stderr).to_string())
                }
            }
            Platform::Windows => Ok("Service start not supported on Windows via schtasks".to_string()),
            Platform::Unknown => Err("Unsupported platform".to_string()),
        }
    }

    pub fn stop() -> Result<String, String> {
        match Platform::detect() {
            Platform::Macos => {
                let output = std::process::Command::new("launchctl")
                    .args(["stop", SERVICE_IDENTIFIER])
                    .output()
                    .map_err(|e| format!("launchctl stop failed: {}", e))?;
                if output.status.success() {
                    Ok("Service stopped".to_string())
                } else {
                    Err(String::from_utf8_lossy(&output.stderr).to_string())
                }
            }
            Platform::Linux => {
                let output = std::process::Command::new("systemctl")
                    .args(["--user", "stop", SERVICE_IDENTIFIER])
                    .output()
                    .map_err(|e| e.to_string())?;
                if output.status.success() {
                    Ok("Service stopped".to_string())
                } else {
                    Err(String::from_utf8_lossy(&output.stderr).to_string())
                }
            }
            Platform::Windows => Ok("Service stop not supported on Windows via schtasks".to_string()),
            Platform::Unknown => Err("Unsupported platform".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let p = Platform::detect();
        let s = p.as_str();
        assert!(s == "macos" || s == "linux" || s == "windows" || s == "unknown");
    }

    #[test]
    fn test_service_identifier() {
        assert!(SERVICE_IDENTIFIER.contains("crawlflow"));
    }

    #[test]
    fn test_data_dir() {
        let d = data_dir();
        assert!(d.to_string_lossy().contains("crawlflow"));
    }

    #[test]
    fn test_plist_generation() {
        let plist = SystemServiceManager::generate_plist("/Applications/CrawlFlow.app/Contents/MacOS/crawlflow");
        assert!(plist.contains("com.crawlflow.desktop-service"));
        assert!(plist.contains("--service"));
        assert!(plist.contains("RunAtLoad"));
        assert!(plist.contains("KeepAlive"));
    }

    #[test]
    fn test_systemd_generation() {
        let unit = SystemServiceManager::generate_systemd("/usr/bin/crawlflow");
        assert!(unit.contains("CrawlFlow Background Service"));
        assert!(unit.contains("--service"));
        assert!(unit.contains("Restart=on-failure"));
    }

    #[test]
    fn test_service_install_info_default() {
        let info = ServiceInstallInfo {
            installed: false,
            running: false,
            auto_start: false,
            service_path: "/tmp/test.plist".into(),
            platform: "macos".into(),
            executable: "/usr/local/bin/crawlflow".into(),
            log_dir: "/tmp/crawlflow/logs".into(),
        };
        assert!(!info.installed);
        assert!(!info.running);
        assert_eq!(info.platform, "macos");
    }
}
