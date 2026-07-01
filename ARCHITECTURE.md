# Kiến trúc CrawlFlow Desktop

## Tổng quan

CrawlFlow Desktop là ứng dụng desktop cho phép người dùng xây dựng visual web crawler pipeline (data source → processor → export) qua giao diện kéo-thả React Flow, với backend xử lý bằng Python trên nền tảng Rust/Tauri.

## Kiến trúc 3 tầng

```
┌─────────────────────────────────────────────────┐
│  Tầng 1: UI (JavaScript/TypeScript + React)     │
│  - React Flow canvas (kéo-thả node)             │
│  - Project Manager (dashboard)                  │
│  - Plugin Manager (enable/disable plugin)       │
│  - Chỉ gọi Tauri invoke(), không xử lý dữ liệu  │
├─────────────────────────────────────────────────┤
│  Tầng 2: Backend API (Rust)                     │
│  - Tauri commands (cầu nối JS ↔ Python)         │
│  - Plugin Engine (Rust built-in processors)     │
│  - HTTP client (reqwest)                        │
│  - HTML parser (scraper)                        │
│  - SQLite (tauri-plugin-sql)                    │
│  - PyO3 bridge (khởi tạo Python interpreter)    │
├─────────────────────────────────────────────────┤
│  Tầng 3: Xử lý (Python)                         │
│  - Plugin scripts (*.py)                        │
│  - Business logic: transform, filter, export    │
│  - Gọi `crawlflow.*` API (Rust → function)     │
└─────────────────────────────────────────────────┘
```

### Luồng dữ liệu

```
User Action (click, drag, config)
  │
  ▼
React Component
  │ invoke('command_name', {args})
  ▼
Rust Command Handler
  │
  ├── Nếu là Python plugin: PythonPluginEngine.call_hook()
  │     │ Python::with_gil() → load script → call function
  │     │ Plugin gọi crawlflow.fetch_url() / extract_html() / ...
  │     ▼
  │   Rust API (crawlflow.* = #[pyfunction])
  │
  ├── Nếu là Rust built-in: execute_processor() trực tiếp
  │
   └── Trả về kết quả (JSON) → Frontend render
```

### Luồng đặc biệt: BeautifulSoup HTML Parse (Python → Rust struct)

```
HTML (raw)
  │
  ▼
Python Plugin (bs4-parser/main.py)
  │ BeautifulSoup(html, "html.parser")
  │ find_all('a', 'img', 'h1-h6', 'meta', 'table', ...)
  ▼
JSON (ParsedHtmlItem array: tag, text, html, type, attributes, href, ...)
  │
  ▼
Rust Command (parse_html_with_bs4_cmd)
  │ serde_json::from_value::<Vec<ParsedHtmlItem>>(json)
  ▼
Rust struct Vec<ParsedHtmlItem> (native Rust objects)
  │
  ├── summarize_parsed_html_cmd() → ParsedHtmlSummary
  │     (links, images, headings, meta_tags, tables, text_blocks)
  │
  └── Có thể xử lý tiếp bằng Rust processors (filter, sort, v.v.)
```

### Luồng xử lý pipeline

```
[Data Source] ──data──▶ [Processor 1] ──data──▶ [Processor 2] ──▶ [Export]
      │                      │                       │
      ▼                      ▼                       ▼
  Rust / Python        Python / Rust            Python / Rust
  (fetch URL,       (transform, filter,     (CSV, JSON, API)
   RSS, API)         deduplicate, sort)
```

---

## Cấu trúc thư mục

```
crawlflow-desktop-app/
├── index.tsx                    # Entry point React
├── App.tsx                      # Root component, view-switching
├── types.ts                     # TypeScript types (NodeData, Plugin, ...)
├── presets.ts                   # Processor presets
├── vite.config.ts               # Vite config (Tauri)
├── package.json                 # JS dependencies
│
├── index.html
├── index.css / tailwind.config.js / postcss.config.js
│
├── components/
│   ├── ProjectManager.tsx       # Dashboard: danh sách project
│   ├── ProjectCard.tsx          # Card hiển thị 1 project
│   ├── CreateProjectForm.tsx    # Form tạo project mới
│   ├── EmptyState.tsx           # Trạng thái rỗng (chưa có project)
│   ├── Sidebar.tsx              # Sidebar: data sources, processors
│   └── PluginManagerPanel.tsx   # Modal quản lý plugin
│
├── lib/
│   ├── db.ts                    # SQLite CRUD (master + project DB)
│   ├── pluginManager.ts         # Plugin registry singleton
│   ├── pythonPlugins.ts         # Bridge frontend ↔ Python plugins
│   └── plugins/
│       └── builtin.ts           # 10 built-in JS plugins (wrapper invoke)
│
├── plugins/                     # Python plugins (runtime directory)
│   ├── json_transformer/
│   │   ├── plugin.json          # Manifest
│   │   └── main.py              # Hook implementations
│   └── bs4-parser/
│       ├── plugin.json          # Manifest
│       └── main.py              # BeautifulSoup HTML parser → Rust struct
│
└── src-tauri/
    ├── Cargo.toml               # Rust dependencies
    ├── tauri.conf.json           # Tauri configuration
    ├── capabilities/             # Tauri v2 capabilities
    └── src/
        ├── main.rs              # Desktop entry point
        ├── lib.rs               # Tauri builder, command registration
        ├── commands.rs           # Tauri command handlers
        ├── models.rs             # Shared Rust types (CrawlRequest, ...)
        ├── crawler.rs            # HTTP fetch + HTML extract (reqwest + scraper)
        ├── plugins.rs            # Plugin engine + built-in Rust processors
        ├── python_plugins.rs     # PyO3 engine + crawlflow API
        └── migrations.rs         # SQLite schema migrations
```

---

## Rust Backend (`src-tauri/src/`)

### `lib.rs` — Application entry point

- Khởi tạo `PluginEngine` với `resolve_plugin_dir()` (~/.local/share/crawlflow/plugins)
- Đăng ký built-in Rust processors
- Cấu hình plugins Tauri (dialog, fs, sql, log)
- Đăng ký toàn bộ command handlers
- Gọi `engine.init_python_plugins()` trong `setup()` để phát hiện Python plugins

### `commands.rs` — Tauri command handlers

| Command | Mô tả | Đường đi |
|---------|-------|----------|
| `fetch_url_cmd` | Fetch URL qua HTTP | Rust `crawler::fetch_url` |
| `batch_crawl_cmd` | Crawl nhiều URL | Rust `crawler::batch_crawl` |
| `extract_html_cmd` | Trích xuất HTML theo rules | Rust `crawler::extract_from_html` |
| `execute_processor_cmd` | Chạy processor (Rust hoặc Python) | `PluginEngine::execute_processor` |
| `list_plugins_cmd` | Danh sách plugin (Rust + Python) | `PluginEngine::list_plugins` |
| `execute_batch_processor_cmd` | Pipeline processors | `PluginEngine::execute_processor` (loop) |
| `fetch_rss_cmd` | Fetch RSS feed | Rust `inner_fetch_rss` |
| `export_csv_cmd` | Export CSV | Rust `inner_export_csv` |
| `parse_html_table_cmd` | Parse HTML table | Rust `inner_parse_html_table` |
| `list_python_plugins_cmd` | Danh sách Python plugins | Python engine |
| `execute_python_hook_cmd` | Gọi hook Python bất kỳ | Python engine |
| `call_python_data_source_cmd` | Gọi `fetch_data` Python | Python engine |
| `call_python_export_cmd` | Gọi `export_data` Python | Python engine |
| `run_python_pipeline_cmd` | Chạy pipeline Python steps | Python engine |
| `reload_python_plugins_cmd` | Reload Python plugins | Python engine |
| `parse_html_with_bs4_cmd` | Parse HTML via BeautifulSoup → Rust struct | Python bs4-parser → serde |
| `summarize_parsed_html_cmd` | Summarize parsed items (Rust-side) | Rust `Vec<ParsedHtmlItem>` → `ParsedHtmlSummary` |

### `python_plugins.rs` — PyO3 Engine

**Struct `PythonPluginEngine`:**
- `discover()` — Quét thư mục plugin, đọc `plugin.json` + `main.py`
- `call_hook()` — Gọi bất kỳ function nào trong plugin Python
  - Serialize input → JSON string → Python → JSON string → Deserialize output
- `call_data_source()` — Gọi `fetch_data(config_json)` Python
- `call_export()` — Gọi `export_data(data_json, config_json)` Python
- `run_pipeline()` — Chạy chuỗi processor steps (prefix `py-`)

**Module Python `crawlflow`** (Rust → Python functions):

| Function | Mô tả |
|----------|-------|
| `crawlflow.fetch_url(url, headers=None)` | HTTP GET → `{"status", "body", "url"}` (JSON string) |
| `crawlflow.log(message, level="info")` | Ghi log |
| `crawlflow.extract_html(html, rules)` | Extract HTML → JSON string array |
| `crawlflow.save_file(path, content)` | Ghi file → bool |
| `crawlflow.read_file(path)` | Đọc file → string |
| `crawlflow.fetch_rss(url, max_items=50)` | Fetch RSS → JSON string array |
| `crawlflow.export_csv(data, delimiter=",")` | Dữ liệu → CSV string |
| `crawlflow.parse_html_table(html, table_index=0, has_header=True)` | Parse HTML table → JSON string array |

**Caching:** Mỗi plugin được compile một lần, globals được giữ trong `Py<PyDict>` để các lần gọi sau không cần parse lại.

### `plugins.rs` — Plugin Engine

- `RustPlugin`: processor với `fn` pointer (deduplicate, filter, sort, limit)
- `PluginEngine`: quản lý cả Rust + Python plugins
- `execute_processor()`: thử Python (`py-` prefix) trước, fallback Rust
- `register_builtin_plugins()`: đăng ký 4 processors (deduplicate, filter, sort, limit)

### `crawler.rs` — HTTP + HTML

- `fetch_url()`: HTTP GET via reqwest, parse response, extract HTML/text
- `extract_from_html()`: CSS selector-based extraction via scraper
- `batch_crawl()`: Concurrent crawl nhiều URL

### `models.rs` — Shared types

`CrawlRequest`, `CrawlResult`, `ExtractRule`, `ExtractedField`, `ProcessRequest`, `ProcessResult`, `ExportRequest`, `ExportResult`, `RssFetchRequest`, `PluginInfo`, `ParseRequest`

**BeautifulSoup parsed data (Python → Rust struct):**
- `ParsedHtmlItem` — Kết quả parse từ BeautifulSoup, gồm `tag`, `text`, `html`, `type`, `attributes`, `href`, `src`, `name`, `selector`, `table_index`, `table_data`
- `ParsedHtmlSummary` — Tổng hợp sau khi xử lý: `total_items`, `links`, `images`, `headings`, `meta_tags`, `tables`, `text_blocks`

Dữ liệu từ Python (JSON) → Rust (`serde_json::from_value`) → native structs → xử lý tiếp.

### `migrations.rs` — SQLite schema

**Master DB** (`crawlflow.db`):
| Table | Mô tả |
|-------|-------|
| `projects` | id, name, description, status, db_path, created_at, updated_at |
| `extensions` | id, name, description, type, config, enabled, installed_at |
| `app_settings` | key, value |

**Per-project DB** (`project_{id}.db`):
| Table | Mô tả |
|-------|-------|
| `nodes` | id, type, label, position, data (JSON), ... |
| `edges` | id, source, target, type, animated, data |
| `crawl_data` | id, source_url, field_name, field_value, raw_data, node_id |
| `crawl_logs` | level, message, node_id |
| `project_settings` | key, value |

---

## Python Plugin System

### Cấu trúc plugin

```
plugins/<plugin_id>/
├── plugin.json    # Manifest (bắt buộc)
└── main.py        # Script (bắt buộc)
```

### `plugin.json`

```json
{
  "id": "json-transformer",
  "name": "JSON Transformer",
  "version": "1.0.0",
  "description": "Transform, filter, and remap JSON data",
  "capabilities": ["processor", "dataSource", "export"]
}
```

`capabilities` hỗ trợ: `processor`, `dataSource`, `parser`, `export`

### Hook functions trong `main.py`

| Hook | Signature | Mô tả |
|------|-----------|-------|
| `on_load(config)` | `(dict) → None` | Gọi khi plugin được load |
| `fetch_data(config_json)` | `(str) → str` (JSON) | Data source |
| `process_data(data_json, config_json)` | `(str, str) → str` (JSON) | Processor |
| `parse_data(data_json, config_json)` | `(str, str) → str` (JSON) | Parser |
| `export_data(data_json, config_json)` | `(str, str) → str` | Export |
| `on_unload()` | `() → None` | Cleanup |

Tất cả dữ liệu trao đổi dưới dạng JSON string. Python plugin tự `json.loads()` / `json.dumps()`.

### BeautifulSoup Plugin (`bs4-parser`)

Plugin này parse HTML bằng Python BeautifulSoup và trả về JSON mà Rust deserializes thành struct:

```python
# Python: bs4-parser/main.py
from bs4 import BeautifulSoup

def process_data(data_json, config_json):
    soup = BeautifulSoup(html, "html.parser")
    results = []
    for a in soup.find_all("a", href=True):
        results.append({
            "tag": "a", "text": a.get_text(strip=True),
            "href": a["href"], "type": "link",
            "attributes": dict(a.attrs),
        })
    return json.dumps(results)
```

```rust
// Rust: nhận JSON từ Python → deserialize thành struct
let items: Vec<ParsedHtmlItem> = serde_json::from_value(
    serde_json::Value::Array(json_result)
)?;
// items[0].tag, items[0].item_type, items[0].attributes, ...
```

Yêu cầu: `pip3 install --user beautifulsoup4` (hoặc bundle trong app).

---

## Frontend (JavaScript/TypeScript)

### `lib/pluginManager.ts`

Singleton quản lý tất cả plugin:
- `register(plugin)` — đăng ký JS/Rust plugin
- `init()` — load trạng thái enabled từ DB + init Python plugins
- `getDataSources()`, `getProcessors()`, `getParsers()` — lấy capability
- `executeHook()` — chạy hook lifecycle

### `lib/pythonPlugins.ts`

Bridge phát hiện Python plugins từ Rust và wrap thành `CrawlFlowPlugin`:
- Gọi `invoke('list_python_plugins_cmd')` → lấy metadata
- Tự động tạo `DataSourceDefinition`, `ProcessorDefinition`, `ParserDefinition`
- Mỗi `fetch`/`process`/`parse` gọi `invoke` tới command tương ứng

**BeautifulSoup helpers:**
- `pythonPluginBridge.parseHtmlWithBs4(html, config?)` → gọi `parse_html_with_bs4_cmd`, trả về `ParsedHtmlItem[]` (Rust struct)
- `pythonPluginBridge.summarizeParsedHtml(items)` → gọi `summarize_parsed_html_cmd`, trả về `ParsedHtmlSummary`

TypeScript types `ParsedHtmlItem` và `ParsedHtmlSummary` mirror chính xác Rust structs qua serde.

### `lib/plugins/builtin.ts`

10 plugins built-in wrap Rust commands qua `invoke()`. Không có logic xử lý trong JS.

### `types.ts` — Plugin types

```typescript
CrawlFlowPlugin     // id, name, capabilities, hooks, dataSource, processor, parser
DataSourceDefinition // type, label, fetch(config) → Promise<any[]>
ProcessorDefinition  // type, label, process(data, config) → Promise<any[]>
ParserDefinition     // id, inputFormats, parse(input, config) → Promise<any[]>
PluginCapability     // 'hook' | 'dataSource' | 'processor' | 'parser'
```

---

## Database

### Master DB (`crawlflow.db`)

Quản lý projects, extensions (plugin enabled state), app settings.
Đường dẫn: `sqlite:crawlflow.db` (Tauri app data dir).

### Per-project DB (`project_{id}.db`)

Mỗi project có database riêng. Lưu nodes, edges (React Flow graph), crawl_data (kết quả crawl), crawl_logs.

---

## Biên dịch & Chạy

### Yêu cầu:
- Rust toolchain (1.77+)
- Python 3.9+ (runtime, headers auto-detected by PyO3)
- Node.js 18+

### Commands:

| Command | Mô tả |
|---------|-------|
| `npm run dev` | Chạy frontend (browser, không Tauri) |
| `npm run tauri dev` | Chạy full app (Tauri desktop) |
| `npm run tauri build` | Build release |
| `cargo check` (trong `src-tauri/`) | Kiểm tra Rust |
| `npx tsc --noEmit` | Kiểm tra TypeScript |

### Lưu ý PyO3:
- Python interpreter khởi tạo tự động qua feature `auto-initialize`
- Nếu không có Python, `init_python_plugins()` log warning, app vẫn chạy bình thường với Rust built-in processors
- Plugin directory: `~/.local/share/crawlflow/plugins/` (Linux/macOS)
- BeautifulSoup plugin yêu cầu `beautifulsoup4` package: `pip3 install --user beautifulsoup4`

---

## Xử lý lỗi & Degrade

- **Không có Python**: App vẫn chạy, processors Rust built-in hoạt động, Python plugins bị ẩn
- **Không có Tauri** (`npm run dev` browser mode): `invoke()` ném lỗi, `isTauriEnv()` catch, app fallback
- **Plugin script lỗi**: Python exception được catch, log warning, pipeline tiếp tục với dữ liệu gốc
- **Crawl thất bại**: HTTP error codes được trả về trong `CrawlResult.error`, không crash app
