use crate::models::*;
use crate::python_plugins::{PythonPluginEngine, PythonPluginMeta, PipelineStep};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct RustPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub execute: fn(data: Vec<serde_json::Value>, config: serde_json::Value) -> Result<Vec<serde_json::Value>, String>,
}

pub struct PluginEngine {
    plugins: HashMap<String, RustPlugin>,
    python_engine: PythonPluginEngine,
}

impl PluginEngine {
    pub fn new(plugin_dir: PathBuf) -> Self {
        Self {
            plugins: HashMap::new(),
            python_engine: PythonPluginEngine::new(plugin_dir),
        }
    }

    pub fn register(&mut self, plugin: RustPlugin) {
        self.plugins.insert(plugin.id.clone(), plugin);
    }

    pub fn get_plugin(&self, id: &str) -> Option<&RustPlugin> {
        self.plugins.get(id)
    }

    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        let mut out: Vec<PluginInfo> = self
            .plugins
            .values()
            .map(|p| PluginInfo {
                id: p.id.clone(),
                name: p.name.clone(),
                version: p.version.clone(),
                description: p.description.clone(),
                capabilities: p.capabilities.clone(),
            })
            .collect();

        // Append Python plugin info
        for py_p in self.python_engine.list_plugins() {
            out.push(PluginInfo {
                id: format!("py-{}", py_p.id),
                name: py_p.name.clone(),
                version: py_p.version.clone(),
                description: py_p.description.clone(),
                capabilities: py_p.capabilities.clone(),
            });
        }

        out
    }

    pub fn execute_processor(
        &mut self,
        processor_type: &str,
        data: Vec<serde_json::Value>,
        config: serde_json::Value,
    ) -> ProcessResult {
        // Try Python plugin (prefixed with "py-")
        if let Some(py_id) = processor_type.strip_prefix("py-") {
            if self.python_engine.get_plugin(py_id).is_some() {
                return match self.python_engine.call_hook(py_id, "process_data", data, config) {
                    Ok(result_data) => ProcessResult {
                        success: true,
                        data: result_data,
                        error: None,
                    },
                    Err(e) => ProcessResult {
                        success: false,
                        data: vec![],
                        error: Some(e),
                    },
                };
            }
        }

        // Fall back to Rust built-in plugin
        if let Some(plugin) = self.plugins.get(processor_type) {
            return match (plugin.execute)(data, config) {
                Ok(result_data) => ProcessResult {
                    success: true,
                    data: result_data,
                    error: None,
                },
                Err(e) => ProcessResult {
                    success: false,
                    data: vec![],
                    error: Some(e),
                },
            };
        }

        ProcessResult {
            success: false,
            data: vec![],
            error: Some(format!("Plugin '{}' not found", processor_type)),
        }
    }

    // ── Python plugin management ────────────────────────────────

    pub fn init_python_plugins(&mut self) -> Result<Vec<String>, String> {
        let discovered = self.python_engine.discover()?;
        log::info!("Discovered {} Python plugin(s): {:?}", discovered.len(), discovered);
        Ok(discovered)
    }

    pub fn list_python_plugins_meta(&mut self) -> Vec<PythonPluginMeta> {
        self.python_engine
            .list_plugins()
            .iter()
            .map(|p| PythonPluginMeta::from(*p))
            .collect()
    }

    pub fn call_python_hook(
        &mut self,
        plugin_id: &str,
        hook_name: &str,
        data: Vec<serde_json::Value>,
        config: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, String> {
        self.python_engine.call_hook(plugin_id, hook_name, data, config)
    }

    pub fn call_python_data_source(
        &mut self,
        plugin_id: &str,
        config: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, String> {
        self.python_engine.call_data_source(plugin_id, config)
    }

    pub fn call_python_export(
        &mut self,
        plugin_id: &str,
        data: Vec<serde_json::Value>,
        config: serde_json::Value,
    ) -> Result<String, String> {
        self.python_engine.call_export(plugin_id, data, config)
    }

    pub fn run_python_pipeline(
        &mut self,
        steps: Vec<PipelineStep>,
        initial_data: Vec<serde_json::Value>,
    ) -> Result<Vec<serde_json::Value>, String> {
        self.python_engine.run_pipeline(steps, initial_data)
    }

    pub fn reload_python_plugins(&mut self) -> Result<Vec<String>, String> {
        self.python_engine = PythonPluginEngine::new(self.python_engine.plugin_dir().clone());
        self.python_engine.discover()
    }
}

// ============================================================
// Built-in Rust plugins
// ============================================================

fn deduplicate_plugin(
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> Result<Vec<serde_json::Value>, String> {
    let field = config.get("field").and_then(|v| v.as_str()).unwrap_or("id");
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for item in data {
        let key = item
            .get(field)
            .map(|v| v.to_string())
            .unwrap_or_default();
        if seen.insert(key) {
            result.push(item);
        }
    }

    Ok(result)
}

fn filter_plugin(
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> Result<Vec<serde_json::Value>, String> {
    let field = config
        .get("field")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let operator = config
        .get("operator")
        .and_then(|v| v.as_str())
        .unwrap_or("equals")
        .to_string();
    let value = config
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if field.is_empty() {
        return Err("Filter field is required".to_string());
    }

    let result: Vec<serde_json::Value> = data
        .into_iter()
        .filter(|item| {
            let item_val = item
                .get(&field)
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();

            match operator.as_str() {
                "equals" => item_val == value,
                "contains" => item_val.contains(&value),
                "starts_with" => item_val.starts_with(&value),
                "ends_with" => item_val.ends_with(&value),
                "not_empty" => !item_val.is_empty(),
                "empty" => item_val.is_empty(),
                "greater_than" => {
                    if let (Ok(a), Ok(b)) = (item_val.parse::<f64>(), value.parse::<f64>()) {
                        a > b
                    } else {
                        item_val > value
                    }
                }
                "less_than" => {
                    if let (Ok(a), Ok(b)) = (item_val.parse::<f64>(), value.parse::<f64>()) {
                        a < b
                    } else {
                        item_val < value
                    }
                }
                _ => true,
            }
        })
        .collect();

    Ok(result)
}

fn sort_plugin(
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> Result<Vec<serde_json::Value>, String> {
    let field = config.get("field").and_then(|v| v.as_str()).unwrap_or("");
    let descending = config
        .get("descending")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if field.is_empty() {
        return Err("Sort field is required".to_string());
    }

    let mut result = data;
    result.sort_by(|a, b| {
        let a_val = a.get(field).map(|v| v.to_string()).unwrap_or_default();
        let b_val = b.get(field).map(|v| v.to_string()).unwrap_or_default();

        if descending {
            b_val.cmp(&a_val)
        } else {
            a_val.cmp(&b_val)
        }
    });

    Ok(result)
}

fn limit_plugin(
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> Result<Vec<serde_json::Value>, String> {
    let count = config
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as usize;
    let offset = config
        .get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    Ok(data.into_iter().skip(offset).take(count).collect())
}

pub fn register_builtin_plugins(engine: &mut PluginEngine) {
    engine.register(RustPlugin {
        id: "rust-deduplicate".to_string(),
        name: "Deduplicate".to_string(),
        version: "1.0.0".to_string(),
        description: "Remove duplicate items based on a field".to_string(),
        capabilities: vec!["processor".to_string()],
        execute: deduplicate_plugin,
    });

    engine.register(RustPlugin {
        id: "rust-filter".to_string(),
        name: "Filter".to_string(),
        version: "1.0.0".to_string(),
        description: "Filter data by field conditions".to_string(),
        capabilities: vec!["processor".to_string()],
        execute: filter_plugin,
    });

    engine.register(RustPlugin {
        id: "rust-sort".to_string(),
        name: "Sort".to_string(),
        version: "1.0.0".to_string(),
        description: "Sort data by a field".to_string(),
        capabilities: vec!["processor".to_string()],
        execute: sort_plugin,
    });

    engine.register(RustPlugin {
        id: "rust-limit".to_string(),
        name: "Limit".to_string(),
        version: "1.0.0".to_string(),
        description: "Limit and offset data rows".to_string(),
        capabilities: vec!["processor".to_string()],
        execute: limit_plugin,
    });
}
