use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use std::path::PathBuf;

/// Serialisable metadata about a Python plugin, sent to the frontend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PythonPluginMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
}

impl From<&PythonPlugin> for PythonPluginMeta {
    fn from(p: &PythonPlugin) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            version: p.version.clone(),
            description: p.description.clone(),
            capabilities: p.capabilities.clone(),
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
    /// Cached globals dict so the script is only compiled once.
    globals: Option<Py<PyDict>>,
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
    plugin_dir: PathBuf,
}

impl PythonPluginEngine {
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugins: HashMap::new(),
            plugin_dir,
        }
    }

    /// Discover Python plugins from the plugin directory.
    /// Each plugin is a subdirectory with a plugin.json manifest and main.py.
    pub fn discover(&mut self) -> Result<Vec<String>, String> {
        let mut discovered = Vec::new();

        let entries = std::fs::read_dir(&self.plugin_dir).map_err(|e| e.to_string())?;
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
                globals: None,
            };

            self.plugins.insert(id.clone(), plugin);
            discovered.push(id);
        }

        Ok(discovered)
    }

    pub fn list_plugins(&self) -> Vec<&PythonPlugin> {
        self.plugins.values().collect()
    }

    pub fn get_plugin(&mut self, id: &str) -> Option<&mut PythonPlugin> {
        self.plugins.get_mut(id)
    }

    pub fn set_plugin_dir(&mut self, dir: PathBuf) {
        self.plugin_dir = dir;
    }

    pub fn plugin_dir(&self) -> &PathBuf {
        &self.plugin_dir
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

    Ok(module.into())
}

#[pyfunction(signature = (url, headers=None))]
fn py_fetch_url(url: String, headers: Option<Vec<(String, String)>>) -> PyResult<String> {

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
}

#[pyfunction(signature = (message, level=None))]
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

#[pyfunction]
fn py_save_file(path: String, content: String) -> PyResult<bool> {
    std::fs::write(&path, &content)
        .map(|_| true)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
}

#[pyfunction]
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
