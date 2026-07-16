use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use std::path::PathBuf;
use crate::models::{ProcessorConfig, ExcelStructure, ExcelColumn};

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
        let api = create_crawlflow_api(py)?;
        globals.set_item("crawlflow", api)?;

        let code_cstr = std::ffi::CString::new(code)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        py.run(&code_cstr, Some(&globals), None)?;

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
                .map_err(|e| format!("Python hook '{}.{}' failed: {}", plugin_id, hook_name, e))?;

            let result_str: String = result
                .extract()
                .map_err(|e| format!("Failed to extract Python result string: {}", e))?;

            serde_json::from_str(&result_str)
                .map_err(|e| format!("Failed to parse Python result JSON: {}", e))
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

        Python::with_gil(|py| -> Result<Vec<serde_json::Value>, String> {
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
                .map_err(|e| format!("Python plugin '{}.fetch_data' failed: {}", plugin_id, e))?;

            let result_str: String = result
                .extract()
                .map_err(|e| format!("Failed to extract Python result string: {}", e))?;

            serde_json::from_str(&result_str)
                .map_err(|e| format!("Failed to parse Python result JSON: {}", e))
        })
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
                .map_err(|e| format!("Python plugin '{}.export_data' failed: {}", plugin_id, e))?;

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
                .map_err(|e| format!("Python plugin '{}.preprocess_data' failed: {}", plugin_id, e))?;

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
                .map_err(|e| format!("Python plugin '{}.filter_data' failed: {}", plugin_id, e))?;

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
        
        for (plugin_id, plugin) in &self.plugins {
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
fn create_crawlflow_api<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyModule>> {
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

    Ok(module.into())
}

#[pyfunction(name = "fetch_url", signature = (url, headers=None))]
fn py_fetch_url(url: String, headers: Option<Vec<(String, String)>>) -> PyResult<String> {

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
    log::info!("[PythonPlugin] [{}] {}", lvl, message);
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
