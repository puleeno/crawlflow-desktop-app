# Hướng dẫn tích hợp Updater cho CrawlFlow Desktop

## Kiến trúc

```
┌──────────────────────┐       GET /api/update/check        ┌──────────────────────┐
│  CrawlFlow Desktop   │ ──── current_version, license_key ──▶  Cloudflare Pages   │
│  (Tauri v2 + Rust)   │       machine_id                   │  Marketplace API     │
│                      │ ◀──── update_available, download_url ──                    │
│  0.1.0 → 1.0.0      │       checksum_sha256               │                      │
└──────────────────────┘                                    └──────────────────────┘
                                │
                                ▼
                         Download .dmg / .msi / .deb
                         Verify checksum
                         Launch installer
```

## Cách 1: Dùng `tauri-plugin-updater` (khuyên dùng)

Tauri v2 có plugin chính thức `tauri-plugin-updater` hỗ trợ sẵn macOS (`.dmg`), Windows (`.msi`), Linux (`.deb`/`.AppImage`).

### Bước 1: Thêm plugin

**`src-tauri/Cargo.toml`**:
```toml
[dependencies]
tauri-plugin-updater = "2"
```

**npm** (frontend):
```bash
npm install @tauri-apps/plugin-updater
```

### Bước 2: Đăng ký plugin

**`src-tauri/src/lib.rs`**:
```rust
use tauri_plugin_updater::UpdaterPlugin;

pub fn run() {
    tauri::Builder::default()
        .plugin(UpdaterPlugin::new()) // thêm dòng này
        .invoke_handler(tauri::generate_handler![...])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Bước 3: Cấu hình trong `tauri.conf.json`

**`src-tauri/tauri.conf.json`**:
```json
{
  "productName": "CrawlFlow",
  "version": "0.1.0",
  "identifier": "com.CrawlFlow.desktop",
  "plugins": {
    "updater": {
      "endpoints": [
        "https://crawlflow.com/api/update/check?current_version={{current_version}}&license_key={{license_key}}&machine_id={{machine_id}}"
      ],
      "pubkey": "your-4242-4242-4242-424242424242" // chỉ cần nếu dùng signature
    }
  }
}
```

> Lưu ý: `tauri-plugin-updater` mặc định gửi request GET và parse JSON response với format:
> ```json
> {
>   "version": "1.0.0",
>   "notes": "Release notes...",
>   "pub_date": "2026-07-04T12:00:00Z",
>   "platforms": {
>     "darwin-aarch64": {
>       "url": "https://download.CrawlFlow.com/v1.0.0/crawlflow_aarch64.dmg",
>       "signature": ""
>     },
>     "darwin-x86_64": {
>       "url": "https://download.CrawlFlow.com/v1.0.0/crawlflow_x64.dmg",
>       "signature": ""
>     },
>     "windows-x86_64": {
>       "url": "https://download.CrawlFlow.com/v1.0.0/crawlflow_x64.msi",
>       "signature": ""
>     }
>   }
> }
> ```
>
> Vì endpoint `/api/update/check` của chúng ta đang dùng format riêng, cần tuỳ chỉnh. Xem Cách 2 bên dưới để có giải pháp chủ động hơn.

### Bước 4: Kích hoạt update từ frontend

**`src/App.tsx`** (hoặc component gốc):
```tsx
import { useEffect } from 'react';
import { check } from '@tauri-apps/plugin-updater';

function App() {
  useEffect(() => {
    // Kiểm tra update khi app start
    const checkUpdate = async () => {
      const update = await check();
      if (update?.available) {
        const confirm = await window.confirm(
          `Bản cập nhật ${update.version} đã sẵn sàng. Tải xuống ngay?`
        );
        if (confirm) {
          await update.downloadAndInstall();
          // restart app
          const { relaunch } = await import('@tauri-apps/api/process');
          await relaunch();
        }
      }
    };
    checkUpdate();
  }, []);
  return <AppContent />;
}
```

---

## Cách 2: Tự implement updater (chủ động hơn)

Cách này cho phép kiểm soát hoàn toàn luồng update và tận dụng endpoint `/api/update/check` đã có.

### Bước 1: Thêm Tauri command

**`src-tauri/src/commands.rs`**:
```rust
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri::Emitter;

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct UpdateCheckRequest {
    pub current_version: String,
    pub license_key: String,
    pub machine_id: String,
}

#[derive(Serialize, Clone)]
#[allow(dead_code)]
pub struct UpdateCheckResponse {
    pub update_available: bool,
    pub latest_version: String,
    pub download_url: Option<String>,
    pub checksum_sha256: Option<String>,
    pub is_required: Option<bool>,
    pub license_valid: bool,
    pub license_tier: Option<String>,
}

#[tauri::command]
pub async fn check_update_cmd(
    app: AppHandle,
    req: UpdateCheckRequest,
) -> Result<UpdateCheckResponse, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://crawlflow.com/api/update/check")
        .query(&[
            ("current_version", &req.current_version),
            ("license_key", &req.license_key),
            ("machine_id", &req.machine_id),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let data: UpdateCheckResponse = resp.json().await.map_err(|e| e.to_string())?;

    // Nếu có bản cập nhật và license hợp lệ, emit event để frontend hiển thị
    if data.update_available && data.license_valid {
        app.emit("update-available", data.clone()).map_err(|e| e.to_string())?;
    }

    Ok(data)
}
```

### Bước 2: Đăng ký command

**`src-tauri/src/lib.rs`** (trong `invoke_handler`):
```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands
    commands::check_update_cmd,
])
```

### Bước 3: Download và cài đặt

**`src-tauri/src/commands.rs`** (thêm command mới):
```rust
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[tauri::command]
pub async fn download_and_install_update(
    app: AppHandle,
    download_url: String,
    checksum: String,
) -> Result<(), String> {
    // 1. Tải file
    let client = reqwest::Client::new();
    let bytes = client
        .get(&download_url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Read response failed: {}", e))?;

    // 2. Verify checksum SHA-256
    let hash = sha2::Sha256::digest(&bytes);
    let hash_hex = format!("{:x}", hash);
    if hash_hex != checksum {
        return Err("Checksum mismatch!".into());
    }

    // 3. Lưu file tạm
    let temp_dir = std::env::temp_dir().join("crawlflow-update");
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    let ext = if cfg!(target_os = "macos") {
        "dmg"
    } else if cfg!(target_os = "windows") {
        "msi"
    } else {
        "deb"
    };

    let installer_path = temp_dir.join(format!("crawlflow_update.{}", ext));
    fs::write(&installer_path, &bytes).map_err(|e| e.to_string())?;

    // 4. Mở installer (hệ điều hành tự xử lý)
    open::that(&installer_path).map_err(|e| format!("Open installer failed: {}", e))?;

    Ok(())
}
```

> Cần thêm crate: `sha2`, `open` vào Cargo.toml.

### Bước 4: Frontend gọi và hiển thị

**`src/UpdaterBanner.tsx`**:
```tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface UpdateInfo {
  latest_version: string;
  download_url: string;
  checksum_sha256: string;
  is_required: boolean;
  license_tier: string;
}

export function UpdaterBanner() {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [machineId] = useState(() => crypto.randomUUID());
  const licenseKey = localStorage.getItem('license_key');

  useEffect(() => {
    const unlisten = listen<UpdateInfo>('update-available', (event) => {
      setUpdate(event.payload);
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  useEffect(() => {
    // Kiểm tra khi app start
    invoke('check_update_cmd', {
      req: {
        current_version: '0.1.0',
        license_key: licenseKey || '',
        machine_id: machineId,
      },
    });
  }, []);

  if (!update) return null;

  const handleUpdate = async () => {
    setDownloading(true);
    try {
      await invoke('download_and_install_update', {
        downloadUrl: update.download_url,
        checksum: update.checksum_sha256,
      });
    } catch (e) {
      console.error('Update failed:', e);
      alert('Cập nhật thất bại. Vui lòng thử lại sau.');
    } finally {
      setDownloading(false);
    }
  };

  return (
    <div style={bannerStyle}>
      <span>📦 Bản cập nhật {update.latest_version} đã sẵn sàng</span>
      <button onClick={handleUpdate} disabled={downloading} style={btnStyle}>
        {downloading ? 'Đang tải...' : 'Cập nhật ngay'}
      </button>
    </div>
  );
}

const bannerStyle: React.CSSProperties = {
  position: 'fixed', bottom: 0, left: 0, right: 0,
  background: 'linear-gradient(135deg, #667eea, #764ba2)',
  color: 'white', padding: '1rem 2rem', display: 'flex',
  justifyContent: 'space-between', alignItems: 'center',
  zIndex: 9999,
};

const btnStyle: React.CSSProperties = {
  background: 'white', color: '#667eea', border: 'none',
  padding: '0.5rem 1.5rem', borderRadius: 8, fontWeight: 700,
  cursor: 'pointer',
};
```

> App.tsx cần mount `<UpdaterBanner />` ở component gốc.

---

## Build và sign file cài đặt (macOS)

### Tạo certificate

Mở **Keychain Access** → **Certificate Assistant** → **Create a Certificate**:
- Name: `CrawlFlow Developer`
- Certificate Type: `Code Signing`
- Luôn để trong login keychain

### Cấu hình Tauri build

**`src-tauri/tauri.conf.json`** (signing section):
```json
{
  "bundle": {
    "macOS": {
      "signingIdentity": "CrawlFlow Developer",
      "providerShortName": "", // bỏ trống nếu Apple ID cá nhân
      "minimumSystemVersion": "13.0"
    }
  }
}
```

### Build DMG

```bash
npm run tauri build  # tự động sign + tạo .dmg
```

Sau build, file `.dmg` nằm ở `src-tauri/target/release/bundle/dmg/`.

### Upload lên Cloudflare R2

Dùng R2 để host file tải về:

```bash
# Cài aws CLI (nếu chưa có)
brew install awscli

# Config endpoint Cloudflare R2
aws s3 ls --endpoint-url https://<account-id>.r2.cloudflarestorage.com

# Upload file
aws s3 cp src-tauri/target/release/bundle/dmg/CrawlFlow_0.1.0_aarch64.dmg \
  s3://crawlflow-items/releases/v0.1.0/crawlflow_aarch64.dmg \
  --endpoint-url https://<account-id>.r2.cloudflarestorage.com
```

Tạo public URL qua R2 bucket → Settings → Public Access → `https://pub-<hash>.r2.dev/releases/v0.1.0/crawlflow_aarch64.dmg`

Hoặc dùng domain riêng: `https://download.CrawlFlow.com/releases/v0.1.0/crawlflow_aarch64.dmg`

---

## Thêm bản ghi update vào database

Sau khi build và upload, thêm bản ghi vào `app_versions`:

```bash
npx wrangler d1 execute crawlflow-db --remote --command="
INSERT INTO app_versions (version, download_url, checksum_sha256, changelog, is_stable, is_required, created_at)
VALUES (
  '1.0.0',
  'https://download.CrawlFlow.com/releases/v1.0.0/crawlflow_aarch64.dmg',
  'sha256-checksum-cua-file',
  '- Thêm license system\n- Cập nhật Inspector UI\n- Fix crash khi start service',
  1,
  0,
  datetime('now')
);
"
```

Tính checksum SHA-256:
```bash
shasum -a 256 src-tauri/target/release/bundle/dmg/CrawlFlow_0.1.0_aarch64.dmg
```

---

## Kiểm tra luồng update hoàn chỉnh

1. **Build app hiện tại** → `npm run tauri build` → ra `CrawlFlow_0.1.0.dmg`
2. **Upload file .dmg lên R2** + thêm `app_versions` record version `1.0.0`
3. **Forward version** trong code lên `1.0.0` → build lại → ra `CrawlFlow_1.0.0.dmg`
4. **Cài app cũ** (0.1.0), mở lên → gọi `/api/update/check` → thấy version `1.0.0` → hiện banner
5. **Click Update** → download file mới, verify checksum, mở installer
6. **Cài xong** → app mới version 1.0.0

```
CrawlFlow 0.1.0  ──check──▶  crawlflow.com/api/update/check
     │                        response: { update_available: true, version: "1.0.0" }
     ▼
 Hiển thị banner "Cập nhật 1.0.0"
     │
     ▼ click Update
 Download DMG → verify SHA-256 → open installer
     │
     ▼ cài đặt xong
CrawlFlow 1.0.0
```

---

## Các crate cần thêm cho custom updater

**`src-tauri/Cargo.toml`**:
```toml
[dependencies]
tauri-plugin-updater = "2"          # Cách 1
sha2 = "0.10"                       # Cách 2 (SHA-256 checksum)
open = "5"                          # Cách 2 (mở file = double click)
```
