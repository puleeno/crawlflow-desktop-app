# Build & Test Workflow

This document explains how CrawlFlow Desktop is built and tested, which
binaries are produced, and the **correct workflow** for running the
pipeline after editing Rust or Python code. Read this before you try to
run a pipeline and wonder why your code changes "don't take effect".

---

## 1. Two separate binaries

The project produces **two** distinct executables. They are built
independently and one launches the other.

| Binary | Produced by | Purpose |
|---------|-------------|---------|
| `CrawlFlow Desktop.app` (GUI) | `npm run tauri dev` / `npm run tauri build` | The React/Tauri desktop window. The thing you see. |
| `crawlflow-service` (headless) | `scripts/build-service.sh` | A background service that runs the crawl pipeline. Declared as a Tauri `externalBin`. |

The GUI app does **not** run the pipeline itself. It writes a control
flag into SQLite (`project_runtime.service_control = 'run'`) and a
**launchd agent** (`~/Library/LaunchAgents/com.CrawlFlow.desktop-service.plist`)
spawns `crawlflow-service`, which reads that flag and executes the
pipeline.

> The service binary lives at
> `src-tauri/binaries/crawlflow-service-<target-triple>`
> (e.g. `crawlflow-service-aarch64-apple-darwin` on Apple Silicon).
> This path is what the launchd plist points at.

---

## 2. `npm run tauri dev` does NOT build the service

This is the single most common source of confusion.

`npm run tauri dev`:
1. Starts the Vite dev server (frontend hot-reload).
2. Compiles the **GUI** Rust binary (`CrawlFlow Desktop.app`).
3. Copies any existing `externalBin` binaries from `src-tauri/binaries/`
   into the app bundle.

It does **not** recompile `crawlflow-service`. If you edit Rust code
in `src-tauri/src/...` and only run `tauri dev`, the GUI updates
but the **service keeps running the OLD binary**. Your changes are
invisible.

### Why the "stale binary" problem happens
While the service (or GUI) is running, it holds the
`src-tauri/binaries/crawlflow-service-*` file open. A subsequent
`cargo build` cannot overwrite it, so the build either fails or the
running process keeps using the old bytes. This is why the build script
kills everything first.

---

## 3. Correct workflow after editing code

### Edit Rust backend (pipeline / worker / plugins / etc.)
```bash
# 1. Stop the running service + GUI so the binary is released
#    (build-service.sh does this for you)
./scripts/build-service.sh

# 2. That script: unloads the launchd agent, pkills the service,
#    compiles crawlflow-service in --release, and COPIES the fresh
#    binary into src-tauri/binaries/crawlflow-service-<triple>
```
`scripts/build-service.sh` is the **only** supported way to rebuild the
service. Do not run `cargo build --bin crawlflow-service` by hand and
forget to copy it into `binaries/` — the script handles both.

Then launch the GUI normally:
```bash
npm run tauri dev
```
The GUI will spawn the freshly-built service from `binaries/`.

### Edit a Python plugin (`plugins/**/main.py`)
Python plugins are loaded at runtime by the service (PyO3 bridge). No
Rust recompile is needed — just **restart the service** so it reloads:
```bash
./scripts/build-service.sh   # also kills + restarts via launchd
# or simply stop/start the project from the GUI
```
(`reload_python_plugins` is also wired to the GUI's reload command.)

### Edit the frontend (`src/`, `*.tsx`)
```bash
npm run tauri dev   # Vite HMR + Tauri rebuild of the GUI
```
The service binary is unaffected, so no service rebuild is required unless
you also touched Rust.

---

## 4. Running tests

Unit / integration tests live in each Rust module (`#[cfg(test)] mod tests`).
Run them with:
```bash
cd src-tauri
cargo test --lib          # all lib tests
cargo test --bin crawlflow-service
```
The integration test `pipeline::tests::test_pipeline_with_oreka_shop`
exercises the full repository pipeline end-to-end (preprocess → worker
match → process → finish actions) against a throwaway SQLite DB.

> Note: the service binary built by `build-service.sh` is `--release`.
> `cargo test` / `cargo build` (without the script) build the
> `target/debug` binary, which is **not** what the GUI launches.
> Always use the script for a binary the GUI will actually run.

---

## 5. Module responsibilities (post-refactor)

The pipeline logic is intentionally **not** a single god-class. Each
concern lives in its own module under `src-tauri/src/`:

| Module | Responsibility |
|---------|----------------|
| `pipeline.rs` | **Orchestrator only.** Owns `PipelineConfig`/`PipelineNode`/`PipelineEdge` graph types, `execute_pipeline`, `execute_repository_pipeline` (the phase loop), topological sort, and fetch helpers. |
| `pipeline_config.rs` | Pure config extraction (no I/O). `extract_preprocessors`, `extract_fetch_data_config`, `extract_pagination_config`, `parse_extract_rules_array`, `build_plugin_config`, `simple_hash`. |
| `worker_engine.rs` | Worker model + execution. `WorkerDef`/`ProcessorStep`, `match_items`, `process_items*`, and `extract_workers` (builds workers from the graph, merging extractor rules). |
| `finish_actions.rs` | Terminal actions after workers finish. `FinishAction` enum, `ActionEngine::execute_actions`, `extract_finish_actions`. |
| `plugins.rs` | Processor dispatch (Rust built-ins + Python bridge). `execute_processor`, `excel_export_plugin` (per-item accumulate), `reset_excel_accumulator`. |
| `data_preprocessor.rs` | Preprocessing (store-ID resolution, URL rewrite, listing extraction). |
| `python_plugins.rs` | PyO3 plugin discovery + hook invocation. |

### Rule of thumb
- A function that **reads node JSON and returns a typed config** belongs in
  `pipeline_config.rs`.
- A function that **runs a worker / matches items / extracts workers from
  the graph** belongs in `worker_engine.rs`.
- A function that **builds terminal export/summary actions** belongs in
  `finish_actions.rs`.
- `pipeline.rs` should only *sequence* these steps, never re-implement them.
