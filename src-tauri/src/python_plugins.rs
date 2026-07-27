use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use crate::models::ProcessorConfig;

/// Registry of Python filters registered via `crawlflow.register_filter(name, func)`.
/// Maps a filter name -> (plugin_id, python_function_name).
/// The Python function is invoked by Rust automatically on the relevant data stage
/// (e.g. `parsed_data` is called on every item's parsed data after extraction).
static FILTER_REGISTRY: LazyLock<Mutex<HashMap<String, (String, String)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Format a Python exception including its full traceback, so plugin failures
/// are debuggable instead of showing only a one-line message.
fn format_pyerr(py: Python<'_>, err: &PyErr) -> String {
    let mut out = format!("{}", err.value(py));

    if let Some(tb) = err.traceback(py) {
        match tb.format() {
            Ok(formatted) => {
                out = format!("{}\nTraceback:\n{}", out, formatted);
            }
            Err(_) => {}
        }
    }

    if let Some(cause) = err.cause(py) {
        out = format!("{}\nCaused by: {}", out, cause.value(py));
    }

    out
}

/// Serialisable metadata about a Python plugin, sent to the frontend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PythonPluginMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub source: PluginSource,
}

impl From<&PythonPlugin> for PythonPluginMeta {
    fn from(p: &PythonPlugin) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            version: p.version.clone(),
            description: p.description.clone(),
            capabilities: p.capabilities.clone(),
            source: p.source.clone(),
        }
    }
}

/// Represents a discovered Python plugin with its metadata and cached module.
pub struct PythonPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub script_path: PathBuf,
    pub source: PluginSource,
    /// Cached globals dict so the script is only compiled once.
    globals: Option<Py<PyDict>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PluginSource {
    BuiltIn,
    User,
}

impl PythonPlugin {
    fn ensure_loaded<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        if let Some(ref cached) = self.globals {
            return Ok(cached.bind(py).clone());
        }

        let code = std::fs::read_to_string(&self.script_path)
            .map_err(|e| pyo3::exceptions::PyFileNotFoundError::new_err(e.to_string()))?;

        let globals = PyDict::new(py);
        let api = create_crawlflow_api(py, &self.id)?;
        globals.set_item("crawlflow", api)?;

        let code_cstr = std::ffi::CString::new(code)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        py.run(&code_cstr, Some(&globals), None)?;

    // Run the optional `on_load` hook so plugins can register filters,
    // presets, etc. Called once, on first load (globals are cached after).
    // Pass an empty config dict so plugins declaring `on_load(config)` work,
    // while those with `on_load(config=None)` accept it too.
    if let Ok(Some(hook)) = globals.get_item("on_load") {
        if hook.is_callable() {
            let empty_cfg = pyo3::types::PyDict::new(py);
            if let Err(e) = hook.call1((empty_cfg,)) {
                log::warn!("Plugin '{}' on_load failed: {}", self.id, format_pyerr(py, &e));
            }
        }
    }

        self.globals = Some(globals.clone().unbind());
        Ok(globals)
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.globals = None;
    }
}

pub struct PythonPluginEngine {
    plugins: HashMap<String, PythonPlugin>,
    builtin_dir: Option<PathBuf>,
    user_dir: PathBuf,
}

impl PythonPluginEngine {
    pub fn new(builtin_dir: Option<PathBuf>, user_dir: PathBuf) -> Self {
        Self {
            plugins: HashMap::new(),
            builtin_dir,
            user_dir,
        }
    }

    fn scan_dir(&mut self, dir: &PathBuf, source: PluginSource, discovered: &mut Vec<String>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("plugin.json");
            let main_path = path.join("main.py");

            if !manifest_path.exists() || !main_path.exists() {
                continue;
            }

            let manifest_content =
                std::fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
            let manifest: serde_json::Value =
                serde_json::from_str(&manifest_content).map_err(|e| e.to_string())?;

            let id = manifest
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if id.is_empty() {
                continue;
            }

            let name = manifest
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string();
            let version = manifest
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.1.0")
                .to_string();
            let description = manifest
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let capabilities: Vec<String> = manifest
                .get("capabilities")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let plugin = PythonPlugin {
                id: id.clone(),
                name,
                version,
                description,
                capabilities,
                script_path: main_path,
                source: source.clone(),
                globals: None,
            };

            // User plugins override built-in plugins with the same ID
            self.plugins.insert(id.clone(), plugin);
            discovered.push(id);
        }

        Ok(())
    }

    /// Discover Python plugins from the user directory first, then built-in.
    /// User plugins override built-in plugins with the same ID.
    pub fn discover(&mut self) -> Result<Vec<String>, String> {
        let mut discovered = Vec::new();

        // Clone paths to avoid borrow conflicts
        let builtin = self.builtin_dir.clone();
        let user = self.user_dir.clone();

        // Built-in plugins first (lower priority)
        if let Some(ref builtin_dir) = builtin {
            self.scan_dir(builtin_dir, PluginSource::BuiltIn, &mut discovered)?;
        }

        // User plugins second (higher priority — overrides built-in IDs)
        self.scan_dir(&user, PluginSource::User, &mut discovered)?;

        Ok(discovered)
    }

    pub fn list_plugins(&self) -> Vec<&PythonPlugin> {
        self.plugins.values().collect()
    }

    /// Eagerly load every discovered plugin so its `on_load` hook runs and
    /// any filters (registered via `crawlflow.register_filter`) become
    /// available immediately. Without this, a filter would only be registered
    /// the first time some other hook (e.g. `process_data`) happened to load
    /// the plugin — which never fires for plugins used purely as a data
    /// source, leaving `call_filter` a no-op.
    pub fn load_all(&mut self) {
        let ids: Vec<String> = self.plugins.keys().cloned().collect();
        for id in ids {
            Python::with_gil(|py| {
                if let Some(plugin) = self.plugins.get_mut(&id) {
                    if let Err(e) = plugin.ensure_loaded(py) {
                        log::warn!("Failed to pre-load plugin '{}': {}", id, e);
                    }
                }
            });
        }
    }

    /// Keep only the plugins enabled in the application's extension settings.
    pub fn retain_plugins(&mut self, enabled_plugin_ids: &std::collections::HashSet<String>) {
        self.plugins
            .retain(|plugin_id, _| enabled_plugin_ids.contains(plugin_id));
    }

    pub fn get_plugin(&mut self, id: &str) -> Option<&mut PythonPlugin> {
        self.plugins.get_mut(id)
    }

    pub fn plugin_dirs(&self) -> (&Option<PathBuf>, &PathBuf) {
        (&self.builtin_dir, &self.user_dir)
    }

    pub fn user_plugin_dir(&self) -> &PathBuf {
        &self.user_dir
    }

    /// Collect presets from all Python plugins that expose a `register_presets()` function.
    /// Returns a flattened list of preset definitions as JSON values.
    pub fn collect_presets(&mut self) -> Vec<serde_json::Value> {
        let mut presets = Vec::new();
        let plugin_ids: Vec<String> = self.plugins.keys().cloned().collect();

        for id in &plugin_ids {
            let plugin = match self.plugins.get_mut(id) {
                Some(p) => p,
                None => continue,
            };

            let result: Result<Vec<serde_json::Value>, String> = Python::with_gil(|py| {
                let globals = plugin
                    .ensure_loaded(py)
                    .map_err(|e| format!("Failed to load plugin: {}", e))?;

                let func = match globals.get_item("register_presets") {
                    Ok(Some(f)) if f.is_callable() => f,
                    _ => return Ok(vec![]),
                };

                let result = func
                    .call0()
                    .map_err(|e| format!("Python plugin '{}.register_presets' failed: {}", id, e))?;

                let result_str: String = result
                    .extract()
                    .map_err(|e| format!("Failed to extract preset string: {}", e))?;

                let parsed: Vec<serde_json::Value> = serde_json::from_str(&result_str)
                    .map_err(|e| format!("Failed to parse preset JSON: {}", e))?;

                Ok(parsed)
            });

            if let Ok(plugin_presets) = result {
                for mut p in plugin_presets {
                    if let Some(obj) = p.as_object_mut() {
                        obj.insert("source".into(), serde_json::Value::String("plugin".into()));
                        obj.insert("plugin_id".into(), serde_json::Value::String(id.clone()));
                    }
                    presets.push(p);
                }
            }
        }

        presets
    }

    // ── Hook helpers ──────────────────────────────────────────────

    /// Call any named function in a Python plugin.
    /// Data is passed as JSON strings (serialised in Rust, deserialised in Python, and back).
    pub fn call_hook(
        &mut self,
        plugin_id: &str,
        hook_name: &str,
        data: Vec<serde_json::Value>,
        config: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, String> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Python plugin '{}' not found", plugin_id))?;

        Python::with_gil(|py| -> Result<Vec<serde_json::Value>, String> {
            let globals = plugin
                .ensure_loaded(py)
                .map_err(|e| format!("Failed to load plugin: {}", e))?;

            let func = match globals.get_item(hook_name) {
                Ok(Some(f)) if f.is_callable() => f,
                _ => return Ok(data),
            };

            // Serialise inputs to JSON strings
            let data_json = serde_json::to_string(&data)
                .map_err(|e| format!("Serialisation error: {}", e))?;
            let config_json = serde_json::to_string(&config)
                .map_err(|e| format!("Serialisation error: {}", e))?;

            let result = func
                .call1((data_json, config_json))
                .map_err(|e| format!("Python hook '{}.{}' failed: {}", plugin_id, hook_name, format_pyerr(py, &e)))?;

            let result_str: String = result
                .extract()
                .map_err(|e| format!("Failed to extract Python result string: {}", e))?;

            serde_json::from_str(&result_str)
                .map_err(|e| format!("Failed to parse Python result JSON: {}", e))
        })
    }

    /// Invoke a registered filter (declared via `crawlflow.register_filter`)
    /// on a batch of parsed data. Returns `None` if no filter with `name` is
    /// registered (caller should keep the original data). If registered but the
    /// plugin fails, the original data is returned unchanged.
    pub fn call_filter(
        &mut self,
        name: &str,
        data: Vec<serde_json::Value>,
    ) -> Option<Vec<serde_json::Value>> {
        let (plugin_id, func_name) = {
            match FILTER_REGISTRY.lock().ok()?.get(name) {
                Some(v) => v.clone(),
                None => return None,
            }
        };

        let plugin = self.plugins.get_mut(&plugin_id)?;

        Python::with_gil(|py| -> Option<Vec<serde_json::Value>> {
            let globals = plugin.ensure_loaded(py).ok()?;
            let func = globals.get_item(&func_name).ok().flatten()?;
            if !func.is_callable() {
                return None;
            }
            let data_json = serde_json::to_string(&data).ok()?;
            let result = func
                .call1((data_json,))
                .map_err(|e| {
                    log::warn!("Python filter '{}' failed: {}", func_name, format_pyerr(py, &e));
                })
                .ok()?;
            let result_str: String = result.extract().ok()?;
            serde_json::from_str(&result_str).ok()
        })
    }

    /// Call `fetch_data` hook in a Python plugin (data source).
    pub fn call_data_source(
        &mut self,
        plugin_id: &str,
        config: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, String> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Python plugin '{}' not found", plugin_id))?;

        let items: Vec<serde_json::Value> = Python::with_gil(|py| -> Result<Vec<serde_json::Value>, String> {
            let globals = plugin
                .ensure_loaded(py)
                .map_err(|e| format!("Failed to load plugin: {}", e))?;

            let func = match globals.get_item("fetch_data") {
                Ok(Some(f)) if f.is_callable() => f,
                _ => return Ok(vec![]),
            };

            let config_json = serde_json::to_string(&config)
                .map_err(|e| format!("Serialisation error: {}", e))?;

            let result = func
                .call1((config_json,))
                .map_err(|e| format!("Python plugin '{}.fetch_data' failed: {}", plugin_id, format_pyerr(py, &e)))?;

            let result_str: String = result
                .extract()
                .map_err(|e| format!("Failed to extract Python result string: {}", e))?;

            serde_json::from_str(&result_str)
                .map_err(|e| format!("Failed to parse Python result JSON: {}", e))
        })?;

        // Apply the reusable "parsed_data" filter (declared by plugins via
        // crawlflow.register_filter) to each emitted item before it is
        // persisted. This is the single choke-point for source plugins that
        // emit product data directly (e.g. oreka's image URL transform),
        // so no hard-coded field mapping is needed in Rust.
        if !items.is_empty() {
            if let Some(filtered) = self.call_filter("parsed_data", items.clone()) {
                return Ok(filtered);
            }
        }

        Ok(items)
    }

    /// Call `export_data` hook in a Python plugin.
    pub fn call_export(
        &mut self,
        plugin_id: &str,
        data: Vec<serde_json::Value>,
        config: serde_json::Value,
    ) -> Result<String, String> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Python plugin '{}' not found", plugin_id))?;

        Python::with_gil(|py| -> Result<String, String> {
            let globals = plugin
                .ensure_loaded(py)
                .map_err(|e| format!("Failed to load plugin: {}", e))?;

            let func = match globals.get_item("export_data") {
                Ok(Some(f)) if f.is_callable() => f,
                _ => return Err("export_data not found".to_string()),
            };

            let data_json = serde_json::to_string(&data)
                .map_err(|e| format!("Serialisation error: {}", e))?;
            let config_json = serde_json::to_string(&config)
                .map_err(|e| format!("Serialisation error: {}", e))?;

            let result = func
                .call1((data_json, config_json))
                .map_err(|e| format!("Python plugin '{}.export_data' failed: {}", plugin_id, format_pyerr(py, &e)))?;

            let output: String = result
                .extract()
                .map_err(|e| format!("Failed to extract Python result string: {}", e))?;

            Ok(output)
        })
    }

    /// Collect preprocessor registrations from all plugins with `register_preprocessors()`.
    pub fn collect_preprocessors(&mut self) -> Vec<crate::data_preprocessor::PreprocessorRegistration> {
        let mut registrations = Vec::new();
        let plugin_ids: Vec<String> = self.plugins.keys().cloned().collect();

        for id in &plugin_ids {
            let result: Result<Vec<crate::data_preprocessor::PreprocessorRegistration>, String> =
                Python::with_gil(|py| {
                    let plugin = match self.plugins.get_mut(id) {
                        Some(p) => p,
                        None => return Ok(vec![]),
                    };

                    let globals = plugin
                        .ensure_loaded(py)
                        .map_err(|e| format!("Failed to load plugin '{}': {}", id, e))?;

                    let func = match globals.get_item("register_preprocessors") {
                        Ok(Some(f)) if f.is_callable() => f,
                        _ => return Ok(vec![]),
                    };

                    let result = func
                        .call0()
                        .map_err(|e| {
                            format!("Python plugin '{}.register_preprocessors' failed: {}", id, e)
                        })?;

                    let result_str: String = result.extract().map_err(|e| {
                        format!("Failed to extract preprocessor string: {}", e)
                    })?;

                    let mut parsed: Vec<crate::data_preprocessor::PreprocessorRegistration> =
                        serde_json::from_str(&result_str).map_err(|e| {
                            format!("Failed to parse preprocessor JSON: {}", e)
                        })?;

                    // Gắn plugin_id cho mỗi registration
                    for reg in &mut parsed {
                        reg.plugin_id = id.clone();
                    }

                    Ok(parsed)
                });

            if let Ok(mut regs) = result {
                registrations.append(&mut regs);
            }
        }

        registrations
    }

    /// Call `preprocess_data` hook in a Python plugin.
    /// Input: serde_json::Value (raw_data, source_url, config)
    /// Output: Vec<NewRawItem>
    pub fn call_preprocessor_hook(
        &mut self,
        plugin_id: &str,
        data: serde_json::Value,
    ) -> Result<Vec<crate::repository::NewRawItem>, String> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Python plugin '{}' not found", plugin_id))?;

        Python::with_gil(|py| -> Result<Vec<crate::repository::NewRawItem>, String> {
            let globals = plugin
                .ensure_loaded(py)
                .map_err(|e| format!("Failed to load plugin: {}", e))?;

            let func = match globals.get_item("preprocess_data") {
                Ok(Some(f)) if f.is_callable() => f,
                _ => return Err("preprocess_data not found".to_string()),
            };

            let data_json =
                serde_json::to_string(&data).map_err(|e| format!("Serialisation error: {}", e))?;

            let result = func
                .call1((data_json,))
                .map_err(|e| format!("Python plugin '{}.preprocess_data' failed: {}", plugin_id, format_pyerr(py, &e)))?;

            let result_str: String = result
                .extract()
                .map_err(|e| format!("Failed to extract Python result string: {}", e))?;

            serde_json::from_str(&result_str)
                .map_err(|e| format!("Failed to parse Python result JSON: {}", e))
        })
    }

    /// Call `filter_data` hook in a Python plugin.
    /// Input: Vec<serde_json::Value> (parsed data), config
    /// Output: Vec<serde_json::Value> (filtered data)
    pub fn call_filter_hook(
        &mut self,
        plugin_id: &str,
        data: Vec<serde_json::Value>,
        config: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, String> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| format!("Python plugin '{}' not found", plugin_id))?;

        Python::with_gil(|py| -> Result<Vec<serde_json::Value>, String> {
            let globals = plugin
                .ensure_loaded(py)
                .map_err(|e| format!("Failed to load plugin: {}", e))?;

            let func = match globals.get_item("filter_data") {
                Ok(Some(f)) if f.is_callable() => f,
                _ => return Ok(data), // If filter function doesn't exist, return data as-is
            };

            let data_json = serde_json::to_string(&data)
                .map_err(|e| format!("Serialisation error: {}", e))?;
            let config_json = serde_json::to_string(&config)
                .map_err(|e| format!("Serialisation error: {}", e))?;

            let result = func
                .call1((data_json, config_json))
                .map_err(|e| format!("Python plugin '{}.filter_data' failed: {}", plugin_id, format_pyerr(py, &e)))?;

            let result_str: String = result
                .extract()
                .map_err(|e| format!("Failed to extract Python result string: {}", e))?;

            serde_json::from_str(&result_str)
                .map_err(|e| format!("Failed to parse Python result JSON: {}", e))
        })
    }

    /// Run a processing pipeline: for each step, delegate to the Python plugin.
    pub fn run_pipeline(
        &mut self,
        steps: Vec<PipelineStep>,
        initial_data: Vec<serde_json::Value>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let mut data = initial_data;
        for step in steps {
            if step.processor_type.starts_with("py-") {
                let py_id = step.processor_type.trim_start_matches("py-");
                data = self.call_hook(py_id, "process_data", data, step.config)?;
            } else {
                return Err(format!(
                    "Non-Python step '{}' should be handled by PluginEngine::execute_processor",
                    step.processor_type
                ));
            }
        }
        Ok(data)
    }

    /// Register processor configurations from Python plugins
    pub fn register_processor_configs(&mut self) -> Result<Vec<ProcessorConfig>, String> {
        let mut configs = Vec::new();
        
        for (plugin_id, plugin) in &mut self.plugins {
            Python::with_gil(|py| -> Result<(), String> {
                let globals = plugin.ensure_loaded(py)
                    .map_err(|e| format!("Failed to load plugin: {}", e))?;
                
                // Check for register_processors function
                let func = match globals.get_item("register_processors") {
                    Ok(Some(f)) if f.is_callable() => f,
                    _ => return Ok(()), // No processor registration function
                };
                
                let result = func.call0()
                    .map_err(|e| format!("Python plugin '{}.register_processors' failed: {}", plugin_id, e))?;
                
                let result_str: String = result.extract()
                    .map_err(|e| format!("Failed to extract Python result string: {}", e))?;
                
                let processor_configs: Vec<serde_json::Value> = serde_json::from_str(&result_str)
                    .map_err(|e| format!("Failed to parse Python result JSON: {}", e))?;
                
                for config_json in processor_configs {
                    if let Ok(config) = serde_json::from_value::<ProcessorConfig>(config_json.clone()) {
                        configs.push(config);
                    }
                }
                
                Ok(())
            })?;
        }
        
        Ok(configs)
    }
}

// ── Pipeline step model ──────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PipelineStep {
    pub processor_type: String,
    pub config: serde_json::Value,
}

// ── Python-accessible API (`crawlflow` module) ───────────────────

/// Create the `crawlflow` Python module exposing Rust backend APIs.
/// `plugin_id` is the id of the plugin currently being loaded, so that
/// `register_filter` can associate a filter with its owning plugin.
fn create_crawlflow_api<'py>(py: Python<'py>, plugin_id: &str) -> PyResult<Bound<'py, PyModule>> {
    let module = PyModule::new(py, "crawlflow")?;

    module.add_function(wrap_pyfunction!(py_fetch_url, py)?)?;
    module.add_function(wrap_pyfunction!(py_log, py)?)?;
    module.add_function(wrap_pyfunction!(py_extract_html, py)?)?;
    module.add_function(wrap_pyfunction!(py_save_file, py)?)?;
    module.add_function(wrap_pyfunction!(py_read_file, py)?)?;
    module.add_function(wrap_pyfunction!(py_fetch_rss, py)?)?;
    module.add_function(wrap_pyfunction!(py_export_csv, py)?)?;
    module.add_function(wrap_pyfunction!(py_parse_html_table, py)?)?;
    module.add_function(wrap_pyfunction!(py_fetch_with_client, py)?)?;
    module.add_function(wrap_pyfunction!(py_update_progress, py)?)?;
    module.add_function(wrap_pyfunction!(py_spreadsheet_read, py)?)?;
    module.add_function(wrap_pyfunction!(py_spreadsheet_write, py)?)?;
    module.add_function(wrap_pyfunction!(py_register_filter, py)?)?;
    module.add_function(wrap_pyfunction!(py_mark_page_done, py)?)?;
    module.add_function(wrap_pyfunction!(py_get_done_pages, py)?)?;
    module.add_function(wrap_pyfunction!(py_save_raw_items, py)?)?;
    module.add_function(wrap_pyfunction!(py_emit_event, py)?)?;

    // Stash the owning plugin id so register_filter knows the caller.
    module.add("__plugin_id", plugin_id)?;

    Ok(module.into())
}

/// `crawlflow.register_filter(name, func)` — register a reusable filter
/// (a "library" function) that Rust invokes automatically on the matching
/// data stage. Example: `crawlflow.register_filter("parsed_data", my_filter)`
/// makes `my_filter(data)` run on every item's parsed data after extraction.
#[pyfunction(name = "register_filter", signature = (name, func))]
fn py_register_filter(name: String, func: Bound<'_, PyAny>) -> PyResult<()> {
    if !func.is_callable() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "register_filter expects a callable function",
        ));
    }
    let globals = func.getattr("__globals__").ok();
    let plugin_id = globals
        .as_ref()
        .and_then(|g| g.get_item("crawlflow").ok())
        .and_then(|cf| cf.getattr("__plugin_id").ok())
        .and_then(|p| p.extract::<String>().ok())
        .unwrap_or_default();

    // Resolve the Python function name from the callable.
    let func_name = func
        .getattr("__name__")
        .ok()
        .and_then(|n| n.extract::<String>().ok())
        .unwrap_or_else(|| "filter".to_string());

    if let Ok(mut reg) = FILTER_REGISTRY.lock() {
        reg.insert(name, (plugin_id, func_name));
    }
    Ok(())
}

#[pyfunction(name = "fetch_url", signature = (url, headers=None, client_type=None, headless=None))]
fn py_fetch_url(
    url: String,
    headers: Option<Vec<(String, String)>>,
    client_type: Option<String>,
    headless: Option<bool>,
) -> PyResult<String> {
    use crate::models::ClientProfile;

    let use_chrome = client_type.as_deref() == Some("chrome")
        || client_type.as_deref() == Some("cdp");

    if use_chrome {
        let profile = ClientProfile {
            client_type: "chrome".into(),
            headless: Some(headless.unwrap_or(true)),
            ..Default::default()
        };
        let (result, _session) = crate::request_clients::fetch_via_cdp(
            &url,
            &profile,
            None,
            None,
            None,
        );
        let body = result.html.unwrap_or_default();
        let status = if result.status == 200 { 200 } else { 0 };
        let out = serde_json::json!({
            "status": status,
            "body": body,
            "url": url,
        });
        return serde_json::to_string(&out)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()));
    }

    let result = std::thread::spawn(move || -> PyResult<String> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        rt.block_on(async {
            let client = reqwest::Client::builder()
                .user_agent("CrawlFlow/1.0")
                .build()
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            let mut builder = client.get(&url);
            if let Some(h) = headers {
                for (k, v) in h {
                    builder = builder.header(&k, &v);
                }
            }

            let resp = builder
                .send()
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();

            let result = serde_json::json!({
                "status": status,
                "body": body,
                "url": url,
            });

            serde_json::to_string(&result)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    })
    .join()
    .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("Thread panicked"))??;

    Ok(result)
}

#[pyfunction(name = "log", signature = (message, level=None))]
fn py_log(message: String, level: Option<String>) {
    let lvl = level.unwrap_or_else(|| "info".to_string());
    crate::logs::log_from_plugin(&lvl, &message);
}

#[pyfunction]
fn py_extract_html(html: String, rules: String) -> PyResult<String> {
    let rules_vec: Vec<serde_json::Value> = serde_json::from_str(&rules)
        .map_err(|e| pyo3::exceptions::PyTypeError::new_err(e.to_string()))?;

    let extract_rules: Vec<crate::models::ExtractRule> = rules_vec
        .iter()
        .map(|r| {
            serde_json::from_value(r.clone()).unwrap_or(crate::models::ExtractRule {
                field: "unknown".into(),
                selector: "".into(),
                attribute: None,
                extract_multiple: None,
                extract_from: None,
                json_path: None,
            })
        })
        .collect();

    let results = crate::crawler::extract_from_html(&html, &extract_rules);
    let json_results: Vec<serde_json::Value> = results
        .iter()
        .map(|r| serde_json::json!({ "field": r.field, "values": r.values }))
        .collect();

    serde_json::to_string(&json_results)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction(name = "save_file")]
fn py_save_file(path: String, content: String) -> PyResult<bool> {
    std::fs::write(&path, &content)
        .map(|_| true)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
}

#[pyfunction(name = "read_file")]
fn py_read_file(path: String) -> PyResult<String> {
    std::fs::read_to_string(&path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
}

#[pyfunction(signature = (url, max_items=None))]
fn py_fetch_rss(url: String, max_items: Option<usize>) -> PyResult<String> {
    let max = max_items.unwrap_or(50);
    let items = crate::commands::inner_fetch_rss(&url, max)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
    serde_json::to_string(&items)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction(signature = (data, delimiter=None))]
fn py_export_csv(data: String, delimiter: Option<String>) -> PyResult<String> {
    let data_vec: Vec<serde_json::Value> = serde_json::from_str(&data)
        .map_err(|e| pyo3::exceptions::PyTypeError::new_err(e.to_string()))?;
    let result = crate::commands::inner_export_csv(&data_vec, &delimiter.unwrap_or_else(|| ",".into()));
    Ok(result)
}

#[pyfunction(signature = (url, client_type="\"reqwest\"", user_agent=None, proxy_url=None, timeout_secs=None, profile_dir=None, chrome_args=None))]
fn py_fetch_with_client(
    url: String,
    client_type: &str,
    user_agent: Option<String>,
    proxy_url: Option<String>,
    timeout_secs: Option<u64>,
    profile_dir: Option<String>,
    chrome_args: Option<Vec<String>>,
) -> PyResult<String> {
    let profile = crate::models::ClientProfile {
        client_type: client_type.to_string(),
        user_agent,
        proxy_url,
        headers: None,
        timeout_secs,
        profile_dir,
        chrome_args,
        wait_for_selector: None,
        extra_nav_args: None,
        headless: Some(true),
    };

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    rt.block_on(async {
        let result = crate::request_clients::fetch_with_client(&url, &profile, None, None, None, None).await;
        let json = serde_json::json!({
            "status": result.status,
            "html": result.html,
            "text": result.text,
            "url": result.url,
            "error": result.error,
        });
        serde_json::to_string(&json)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    })
}

#[pyfunction(signature = (html, table_index=None, has_header=None))]
fn py_parse_html_table(
    html: String,
    table_index: Option<usize>,
    has_header: Option<bool>,
) -> PyResult<String> {
    let result = crate::commands::inner_parse_html_table(
        &html,
        table_index.unwrap_or(0),
        has_header.unwrap_or(true),
    );
    serde_json::to_string(&result)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction(name = "update_progress")]
fn py_update_progress(project_id: String, data: String) -> PyResult<()> {
    let info: crate::progress::ProgressInfo = serde_json::from_str(&data)
        .map_err(|e| pyo3::exceptions::PyTypeError::new_err(e.to_string()))?;
    crate::progress::update_progress(&project_id, info);
    Ok(())
}

/// Resolve a project's per-project SQLite DB path from its id.
fn project_db_path_for(project_id: &str) -> std::path::PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.CrawlFlow.desktop")
        .join(format!("project_{}.db", project_id))
}

/// Mark a crawled listing page as done so a resumed run can skip it.
#[pyfunction(name = "mark_page_done")]
fn py_mark_page_done(
    project_id: String,
    page_url: String,
    page_number: i64,
    item_count: i64,
) -> PyResult<()> {
    let db_path = project_db_path_for(&project_id);
    crate::repository::mark_page_done_by_path(&db_path, &page_url, page_number, item_count)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
}

/// Return the set of already-completed listing page numbers for a project.
#[pyfunction(name = "get_done_pages")]
fn py_get_done_pages(project_id: String) -> PyResult<Vec<i64>> {
    let db_path = project_db_path_for(&project_id);
    Ok(crate::repository::get_done_pages(&db_path).into_iter().collect())
}

/// Save raw items into the project repository immediately, so the UI progress
/// (pending count) updates in real time while a plugin is still collecting URLs.
///
/// `items_json` must be a JSON array of objects with at least `source_url`,
/// `item_type` and `item_hash`. Returns a JSON object `{ inserted, duplicated }`.
#[pyfunction(name = "save_raw_items")]
fn py_save_raw_items(project_id: String, db_path: String, items_json: String) -> PyResult<String> {
    let items: Vec<crate::repository::NewRawItem> = serde_json::from_str(&items_json)
        .map_err(|e| pyo3::exceptions::PyTypeError::new_err(e.to_string()))?;
    let repo = crate::repository::RawItemRepository::open(&std::path::PathBuf::from(&db_path))
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
    let result = repo
        .save_items(&items)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))?;
    // Realtime push: each saved batch is a per-item event so the UI progress
    // bar climbs instantly instead of waiting for the DB poll.
    if let Some(hub) = crate::ws::global_hub() {
        hub.publish(
            &project_id,
            &crate::ws::WsMessage::item(serde_json::json!({
                "event": "items_saved",
                "inserted": result.inserted,
                "duplicated": result.duplicated,
                "total": items.len(),
            })),
        );
    }
    Ok(serde_json::json!({
        "inserted": result.inserted,
        "duplicated": result.duplicated,
    })
    .to_string())
}

/// Push a realtime event to connected WebSocket clients for this project.
/// `event_type` is a free-form string (e.g. "progress", "item", "status").
/// `payload_json` must be a JSON object/array. Used by plugins that want to
/// drive the UI progress bar directly without polling.
#[pyfunction(name = "emit_event")]
fn py_emit_event(project_id: String, event_type: String, payload_json: String) -> PyResult<()> {
    let payload: serde_json::Value = serde_json::from_str(&payload_json)
        .map_err(|e| pyo3::exceptions::PyTypeError::new_err(e.to_string()))?;
    if let Some(hub) = crate::ws::global_hub() {
        let msg = match event_type.as_str() {
            "progress" => crate::ws::WsMessage::progress(payload),
            "log" => crate::ws::WsMessage::log(payload),
            "item" => crate::ws::WsMessage::item(payload),
            "status" => crate::ws::WsMessage::status(payload),
            other => crate::ws::WsMessage { r#type: other.to_string(), payload },
        };
        hub.publish(&project_id, &msg);
    }
    Ok(())
}

// ── Spreadsheet API ──────────────────────────────────────────────

#[pyfunction]
fn py_spreadsheet_read(path: String) -> PyResult<String> {
    let workbook = crate::spreadsheet::read(&path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e))?;
    serde_json::to_string(&workbook)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction]
fn py_spreadsheet_write(data: String, path: String) -> PyResult<bool> {
    let workbook: crate::spreadsheet::Workbook = serde_json::from_str(&data)
        .map_err(|e| pyo3::exceptions::PyTypeError::new_err(e.to_string()))?;
    crate::spreadsheet::write(&workbook, &path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e))?;
    Ok(true)
}

pub fn resolve_python_path() -> Option<std::path::PathBuf> {
    let db_path = crate::commands::master_db_path();
    if db_path.exists() {
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            if let Ok(mut stmt) =
                conn.prepare("SELECT value FROM app_settings WHERE key = 'python_path'")
            {
                if let Ok(row) = stmt.query_row([], |r| r.get::<_, String>(0)) {
                    let p = std::path::PathBuf::from(&row);
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }
    }

    let python_cmd = if cfg!(target_os = "windows") {
        "python"
    } else {
        "python3"
    };
    if let Ok(output) = std::process::Command::new(python_cmd)
        .args(["-c", "import sys; print(sys.prefix)"])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                let p = std::path::PathBuf::from(&path);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }

    None
}
