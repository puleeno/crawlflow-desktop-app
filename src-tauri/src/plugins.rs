use crate::models::*;
use crate::python_plugins::{PipelineStep, PythonPluginEngine, PythonPluginMeta};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct RustPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub presets: Vec<serde_json::Value>,
    pub execute: fn(
        data: Vec<serde_json::Value>,
        config: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, String>,
}

pub struct PluginEngine {
    plugins: HashMap<String, RustPlugin>,
    aliases: HashMap<String, String>,
    python_engine: PythonPluginEngine,
}

impl PluginEngine {
    pub fn new(builtin_dir: Option<PathBuf>, user_dir: PathBuf) -> Self {
        Self {
            plugins: HashMap::new(),
            aliases: HashMap::new(),
            python_engine: PythonPluginEngine::new(builtin_dir, user_dir),
        }
    }

    pub fn register(&mut self, plugin: RustPlugin) {
        let id = plugin.id.clone();
        self.plugins.insert(id.clone(), plugin);
    }

    pub fn register_alias(&mut self, alias: &str, target_id: &str) {
        self.aliases.insert(alias.to_string(), target_id.to_string());
    }

    #[allow(dead_code)]
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
        // Resolve alias
        let resolved = self
            .aliases
            .get(processor_type)
            .map(|s| s.as_str())
            .unwrap_or(processor_type)
            .to_string();

        // Try Python plugin (prefixed with "py-")
        if let Some(py_id) = resolved.strip_prefix("py-") {
            if self.python_engine.get_plugin(py_id).is_some() {
                return match self
                    .python_engine
                    .call_hook(py_id, "process_data", data, config)
                {
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

        // Dispatch to registered Rust plugin
        if let Some(plugin) = self.plugins.get(&resolved) {
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
        log::info!(
            "Discovered {} Python plugin(s): {:?}",
            discovered.len(),
            discovered
        );
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
        self.python_engine
            .call_hook(plugin_id, hook_name, data, config)
    }

    pub fn call_filter_hook(
        &mut self,
        plugin_id: &str,
        data: Vec<serde_json::Value>,
        config: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, String> {
        self.python_engine
            .call_filter_hook(plugin_id, data, config)
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
        let (builtin, user) = self.python_engine.plugin_dirs();
        self.python_engine = PythonPluginEngine::new(builtin.clone(), user.clone());
        self.python_engine.discover()
    }

    // ── Preprocessors ─────────────────────────────────────────────

    /// Collect preprocessor registrations từ tất cả Python plugins
    #[allow(dead_code)]
    pub fn list_preprocessors(&mut self) -> Vec<crate::data_preprocessor::PreprocessorRegistration> {
        self.python_engine.collect_preprocessors()
    }

    /// Execute preprocessor: dispatch to plugin's preprocess_data hook hoặc fallback
    #[allow(dead_code)]
    pub fn execute_preprocessor(
        &mut self,
        raw_data: &str,
        source_url: &str,
        config: &crate::data_preprocessor::PreprocessorConfig,
    ) -> crate::data_preprocessor::PreprocessorResult {
        crate::data_preprocessor::DataPreprocessor::process_with_plugins(
            raw_data,
            source_url,
            config,
            &mut self.python_engine,
        )
    }
}



impl PluginEngine {
    // ── Presets ───────────────────────────────────────────────────

    pub fn list_presets(&mut self) -> Vec<serde_json::Value> {
        let mut presets = Vec::new();

        // Collect presets from registered Rust plugins
        for plugin in self.plugins.values() {
            for preset in &plugin.presets {
                presets.push(preset.clone());
            }
        }

        // Collect presets from Python plugins
        for p in self.python_engine.collect_presets() {
            presets.push(p);
        }

        presets
    }
}

// ── Centralized alias resolution ──────────────────────────
/// Resolve a processor-type alias to its canonical plugin ID.
/// This is the single source of truth for all alias mappings.
/// Returns the input unchanged if no alias is registered.
pub fn resolve_processor_alias(processor_type: &str) -> &str {
    match processor_type {
        "deduplicate" => "rust-deduplicate",
        "filter" => "rust-filter",
        "sort" => "rust-sort",
        "limit" => "rust-limit",
        "excel-export" | "generate-excel-file" => "rust-excel-export",
        _ => processor_type,
    }
}

/// Check whether a processor type is an export-oriented plugin
/// (one that writes output files rather than transforming data).
pub fn is_export_processor(processor_type: &str) -> bool {
    matches!(
        resolve_processor_alias(processor_type),
        "rust-excel-export"
    )
}

/// Execute a processor by type, resolving aliases and dispatching to
/// the correct built-in implementation. Used from contexts that do not
/// hold a `PluginEngine` reference (e.g. the repository pipeline).
pub fn execute_processor_simple(
    processor_type: &str,
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> ProcessResult {
    let resolved = resolve_processor_alias(processor_type);
    let result = match resolved {
        "rust-deduplicate" => deduplicate_plugin(data, config),
        "rust-filter" => filter_plugin(data, config),
        "rust-sort" => sort_plugin(data, config),
        "rust-limit" => limit_plugin(data, config),
        "rust-excel-export" => excel_export_plugin(data, config),
        other => {
            // Unknown type – pass data through (legacy behaviour for
            // frontend-side types like generate-csv-file, save-to-database).
            return ProcessResult {
                success: true,
                data,
                error: Some(format!("Unknown processor type '{}' – passed through", other)),
            };
        }
    };
    match result {
        Ok(data) => ProcessResult {
            success: true,
            data,
            error: None,
        },
        Err(e) => ProcessResult {
            success: false,
            data: vec![],
            error: Some(e),
        },
    }
}

// ============================================================
// Built-in Rust plugins
// ============================================================

pub fn deduplicate_plugin(
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> Result<Vec<serde_json::Value>, String> {
    let field = config.get("field").and_then(|v| v.as_str()).unwrap_or("id");
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();

    for item in data {
        let key = item.get(field).map(|v| v.to_string()).unwrap_or_default();
        if seen.insert(key) {
            result.push(item);
        }
    }

    Ok(result)
}

pub fn filter_plugin(
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

pub fn sort_plugin(
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

pub fn limit_plugin(
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> Result<Vec<serde_json::Value>, String> {
    let count = config.get("count").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
    let offset = config.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    Ok(data.into_iter().skip(offset).take(count).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deduplicate_by_field() {
        let data = vec![
            serde_json::json!({"id": 1, "name": "a"}),
            serde_json::json!({"id": 2, "name": "b"}),
            serde_json::json!({"id": 1, "name": "c"}),
        ];
        let config = serde_json::json!({"field": "id"});
        let result = deduplicate_plugin(data, config).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["name"], "a");
        assert_eq!(result[1]["name"], "b");
    }

    #[test]
    fn test_deduplicate_default_field() {
        let data = vec![
            serde_json::json!({"id": 1}),
            serde_json::json!({"id": 2}),
            serde_json::json!({"id": 1}),
        ];
        let config = serde_json::json!({});
        let result = deduplicate_plugin(data, config).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_equals() {
        let data = vec![
            serde_json::json!({"name": "apple", "color": "red"}),
            serde_json::json!({"name": "banana", "color": "yellow"}),
            serde_json::json!({"name": "cherry", "color": "red"}),
        ];
        let config = serde_json::json!({"field": "color", "operator": "equals", "value": "red"});
        let result = filter_plugin(data, config).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["name"], "apple");
        assert_eq!(result[1]["name"], "cherry");
    }

    #[test]
    fn test_filter_greater_than() {
        let data = vec![
            serde_json::json!({"val": 1}),
            serde_json::json!({"val": 5}),
            serde_json::json!({"val": 10}),
        ];
        let config = serde_json::json!({"field": "val", "operator": "greater_than", "value": "5"});
        let result = filter_plugin(data, config).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["val"], 10);
    }

    #[test]
    fn test_filter_empty_field_error() {
        let data = vec![serde_json::json!({"a": 1})];
        let config = serde_json::json!({"field": "", "operator": "equals", "value": "x"});
        let result = filter_plugin(data, config);
        assert!(result.is_err());
    }

    #[test]
    fn test_filter_not_empty() {
        let data = vec![
            serde_json::json!({"name": "hello"}),
            serde_json::json!({"name": ""}),
            serde_json::json!({"name": "world"}),
        ];
        let config = serde_json::json!({"field": "name", "operator": "not_empty"});
        let result = filter_plugin(data, config).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_contains() {
        let data = vec![
            serde_json::json!({"title": "Hello World"}),
            serde_json::json!({"title": "Goodbye World"}),
            serde_json::json!({"title": "Foo Bar"}),
        ];
        let config =
            serde_json::json!({"field": "title", "operator": "contains", "value": "World"});
        let result = filter_plugin(data, config).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_sort_ascending() {
        let data = vec![
            serde_json::json!({"name": "zebra"}),
            serde_json::json!({"name": "apple"}),
            serde_json::json!({"name": "banana"}),
        ];
        let config = serde_json::json!({"field": "name", "descending": false});
        let result = sort_plugin(data, config).unwrap();
        assert_eq!(result[0]["name"], "apple");
        assert_eq!(result[1]["name"], "banana");
        assert_eq!(result[2]["name"], "zebra");
    }

    #[test]
    fn test_sort_descending() {
        let data = vec![
            serde_json::json!({"name": "apple"}),
            serde_json::json!({"name": "zebra"}),
            serde_json::json!({"name": "banana"}),
        ];
        let config = serde_json::json!({"field": "name", "descending": true});
        let result = sort_plugin(data, config).unwrap();
        assert_eq!(result[0]["name"], "zebra");
        assert_eq!(result[2]["name"], "apple");
    }

    #[test]
    fn test_sort_empty_field_error() {
        let data = vec![serde_json::json!({"a": 1})];
        let config = serde_json::json!({"field": ""});
        let result = sort_plugin(data, config);
        assert!(result.is_err());
    }

    #[test]
    fn test_limit_basic() {
        let data = vec![
            serde_json::json!({"id": 1}),
            serde_json::json!({"id": 2}),
            serde_json::json!({"id": 3}),
            serde_json::json!({"id": 4}),
            serde_json::json!({"id": 5}),
        ];
        let config = serde_json::json!({"count": 3, "offset": 0});
        let result = limit_plugin(data, config).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["id"], 1);
        assert_eq!(result[2]["id"], 3);
    }

    #[test]
    fn test_limit_with_offset() {
        let data = vec![
            serde_json::json!({"id": 1}),
            serde_json::json!({"id": 2}),
            serde_json::json!({"id": 3}),
            serde_json::json!({"id": 4}),
            serde_json::json!({"id": 5}),
        ];
        let config = serde_json::json!({"count": 2, "offset": 2});
        let result = limit_plugin(data, config).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["id"], 3);
        assert_eq!(result[1]["id"], 4);
    }

    #[test]
    fn test_limit_default_count() {
        let data = (0..200).map(|i| serde_json::json!({"id": i})).collect();
        let config = serde_json::json!({});
        let result = limit_plugin(data, config).unwrap();
        assert_eq!(result.len(), 100); // default count
    }

    #[test]
    fn test_register_builtin_plugins() {
        let mut engine = PluginEngine::new(None, PathBuf::from("/tmp"));
        register_builtin_plugins(&mut engine);
        let plugins = engine.list_plugins();
        assert!(plugins.len() >= 5);
        let ids: Vec<_> = plugins.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"rust-deduplicate"));
        assert!(ids.contains(&"rust-filter"));
        assert!(ids.contains(&"rust-sort"));
        assert!(ids.contains(&"rust-limit"));
        assert!(ids.contains(&"rust-excel-export"));
    }
}

pub fn register_builtin_plugins(engine: &mut PluginEngine) {
    engine.register(RustPlugin {
        id: "rust-deduplicate".to_string(),
        name: "Deduplicate".to_string(),
        version: "1.0.0".to_string(),
        description: "Remove duplicate items based on a field".to_string(),
        capabilities: vec!["processor".to_string()],
        presets: vec![],
        execute: deduplicate_plugin,
    });

    engine.register(RustPlugin {
        id: "rust-filter".to_string(),
        name: "Filter".to_string(),
        version: "1.0.0".to_string(),
        description: "Filter data by field conditions".to_string(),
        capabilities: vec!["processor".to_string()],
        presets: vec![],
        execute: filter_plugin,
    });

    engine.register(RustPlugin {
        id: "rust-sort".to_string(),
        name: "Sort".to_string(),
        version: "1.0.0".to_string(),
        description: "Sort data by a field".to_string(),
        capabilities: vec!["processor".to_string()],
        presets: vec![],
        execute: sort_plugin,
    });

    engine.register(RustPlugin {
        id: "rust-limit".to_string(),
        name: "Limit".to_string(),
        version: "1.0.0".to_string(),
        description: "Limit and offset data rows".to_string(),
        capabilities: vec!["processor".to_string()],
        presets: vec![],
        execute: limit_plugin,
    });

    engine.register(RustPlugin {
        id: "rust-excel-export".to_string(),
        name: "Excel Export".to_string(),
        version: "1.0.0".to_string(),
        description: "Export data to Excel (.xlsx) format".to_string(),
        capabilities: vec!["processor".to_string(), "export".to_string()],
        presets: vec![],
        execute: excel_export_plugin,
    });

    engine.register_alias("deduplicate", "rust-deduplicate");
    engine.register_alias("filter", "rust-filter");
    engine.register_alias("sort", "rust-sort");
    engine.register_alias("limit", "rust-limit");
    engine.register_alias("excel-export", "rust-excel-export");
    engine.register_alias("generate-excel-file", "rust-excel-export");
}

pub fn excel_export_plugin(
    data: Vec<serde_json::Value>,
    config: serde_json::Value,
) -> Result<Vec<serde_json::Value>, String> {
    use crate::commands::inner_export_excel;

    let sheet_name = config
        .get("sheetName")
        .and_then(|v| v.as_str())
        .unwrap_or("Sheet1");
    let include_header = config
        .get("includeHeader")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let column_mapping = config.get("columnMapping").and_then(|v| v.as_object());

    // Only keep fields that belong to the worker's Data Extractor settings.
    // `extractFields` is the list of field names produced by the extractor
    // (the `name` of each custom rule / preset rule). DB metadata fields
    // (id, url, source_url, extracted_url, item_type, html, text) must NOT
    // appear in the exported spreadsheet.
    let extract_fields: Option<Vec<String>> = config
        .get("extractFields")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

    // Map of source field name -> export column header (after column mapping).
    let mapped_keys: std::collections::BTreeSet<String> = if let Some(mapping) = column_mapping {
        mapping.keys().cloned().collect()
    } else {
        std::collections::BTreeSet::new()
    };

    let mapped_data: Vec<serde_json::Value> = data
        .iter()
        .map(|item| {
            if let serde_json::Value::Object(obj) = item {
                let mut new_obj = serde_json::Map::new();
                // When extractFields is provided, the Excel columns must follow
                // exactly the extractor settings (name + order). Only those
                // fields are emitted, in that order, so the header matches the
                // worker's Data Extractor configuration.
                if let Some(fields) = extract_fields.as_ref() {
                    for f in fields {
                        // Allow a field to be remapped via columnMapping.
                        let src_key = column_mapping
                            .and_then(|m| {
                                m.iter()
                                    .find(|(_, h)| h.as_str() == Some(f.as_str()))
                                    .map(|(k, _)| k.clone())
                            })
                            .unwrap_or_else(|| f.clone());
                        let val = obj
                            .get(&src_key)
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);
                        new_obj.insert(f.clone(), val);
                    }
                    // Always keep source_url for traceability if present.
                    if let Some(su) = obj.get("source_url") {
                        new_obj.insert("source_url".to_string(), su.clone());
                    }
                } else {
                    for (k, v) in obj.iter() {
                        let is_metadata = matches!(
                            k.as_str(),
                            "id" | "url" | "source_url" | "extracted_url" | "item_type"
                                | "html" | "text" | "status"
                        );
                        let allowed_by_mapping = mapped_keys.contains(k);
                        if is_metadata && !allowed_by_mapping {
                            continue;
                        }
                        let new_key = column_mapping
                            .and_then(|m| m.get(k))
                            .and_then(|v| v.as_str())
                            .unwrap_or(k);
                        new_obj.insert(new_key.to_string(), v.clone());
                    }
                }
                serde_json::Value::Object(new_obj)
            } else {
                item.clone()
            }
        })
        .collect();

    log::info!(
        "[excel_export_plugin] input={} items; extractFields={:?}; mapped_data[0]={}",
        data.len(),
        extract_fields,
        mapped_data
            .first()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "<empty>".into())
    );

    let raw_file_name = config
        .get("fileName")
        .and_then(|v| v.as_str())
        .unwrap_or("export.xlsx");

    // Substitute template tokens ({{date}}, {{timestamp}}) in the file name.
    let date_str = chrono_date();
    let file_name = raw_file_name
        .replace("{{date}}", &date_str)
        .replace("{{timestamp}}", &chrono_now());

    // ── Output directory ──────────────────────────────────────────────
    // Honour an explicit `outputDir` from config (set by the service / pipeline
    // from the global `app_settings.export_dir`). Fall back to the OS Downloads
    // folder, then to the legacy data-dir/exports location.
    let output_dir: std::path::PathBuf = match config.get("outputDir").and_then(|v| v.as_str()) {
        Some(dir) if !dir.trim().is_empty() => std::path::PathBuf::from(dir),
        _ => dirs_next::download_dir()
            .or_else(|| dirs_next::data_dir())
            .unwrap_or_else(|| std::path::PathBuf::from(".")),
    };

    // ── Per-project grouping ──────────────────────────────────────────
    // `groupExport` (bool) + `groupFormat` ("id" | "name" | "both") create a
    // per-project subfolder. `projectId` is always appended to the file name
    // so files never collide across projects even when grouping is off.
    let group_export = config
        .get("groupExport")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let group_format = config
        .get("groupFormat")
        .and_then(|v| v.as_str())
        .unwrap_or("id");
    let project_id = config.get("projectId").and_then(|v| v.as_str()).unwrap_or("");
    let project_name = config.get("projectName").and_then(|v| v.as_str()).unwrap_or("");

    // ── Multi-value serialization ────────────────────────────────────
    let multi_value_mode = config
        .get("multiValueMode")
        .and_then(|v| v.as_str())
        .unwrap_or("separator");
    let multi_value_separator = config
        .get("multiValueSeparator")
        .and_then(|v| v.as_str())
        .unwrap_or(";");
    let cell_opts = crate::spreadsheet::CellOpts {
        separator: multi_value_separator.to_string(),
        mode: crate::spreadsheet::MultiValueMode::from_str(multi_value_mode),
    };

    let mut out_dir = output_dir.clone();
    if group_export && !project_id.is_empty() {
        let label = match group_format {
            "name" if !project_name.is_empty() => sanitize_folder_name(project_name),
            "both" if !project_name.is_empty() => {
                format!("{}-{}", sanitize_folder_name(project_name), project_id)
            }
            _ => project_id.to_string(),
        };
        out_dir = out_dir.join(label);
    }

    // Append the project id to the file name (before the extension) so outputs
    // from different projects never overwrite each other.
    let file_name = if !project_id.is_empty() {
        append_project_id_to_filename(&file_name, project_id)
    } else {
        file_name
    };

    std::fs::create_dir_all(&out_dir).map_err(|e| {
        format!(
            "Failed to create exports dir '{}': {}",
            out_dir.to_string_lossy(),
            e
        )
    })?;

    let out_path = out_dir.join(file_name);

    // Write exactly the rows passed in this call. No cross-cycle accumulation:
    // the service invokes the export once per run with the full dataset, so
    // appending would duplicate rows and shift data to the wrong row index.
    let all_rows = mapped_data;

    let bytes = inner_export_excel(&all_rows, sheet_name, include_header, &cell_opts)
        .map_err(|e| format!("Excel generation failed ({} rows): {}", all_rows.len(), e))?;

    std::fs::write(&out_path, &bytes).map_err(|e| {
        format!(
            "Failed to write Excel file '{}': {}",
            out_path.to_string_lossy(),
            e
        )
    })?;
    log::info!(
        "[excel_export_plugin] appended {} row(s) -> {} (total {} rows, {} bytes)",
        all_rows.len(),
        out_path.to_string_lossy(),
        all_rows.len(),
        bytes.len()
    );

    Ok(vec![serde_json::json!({
        "success": true,
        "file": out_path.to_string_lossy().to_string(),
        "count": all_rows.len(),
        "format": "xlsx"
    })])
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn chrono_date() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let mut year = 1970u32;
    let mut remaining_days = days as u32;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let days_in_year = if leap { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let month_days: &[u32] = if leap {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &d in month_days {
        if remaining_days < d {
            break;
        }
        remaining_days -= d;
        month += 1;
    }
    let day = remaining_days + 1;
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Replace filesystem-unsafe characters in a project name so it can be used
/// as a folder name. Collapses whitespace and keeps it short.
fn sanitize_folder_name(name: &str) -> String {
    let trimmed = name.trim();
    let mut out = String::with_capacity(trimmed.len());
    for c in trimmed.chars() {
        if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' || c == '.' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    // Collapse runs of spaces/underscores into a single dash.
    let out = out.split_whitespace().collect::<Vec<&str>>().join("-");
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "project".to_string()
    } else {
        out
    }
}

/// Insert ` -<project_id>` before the file extension so outputs from different
/// projects never collide. Files without an extension get the suffix appended.
fn append_project_id_to_filename(file_name: &str, project_id: &str) -> String {
    match std::path::Path::new(file_name).extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let stem = std::path::Path::new(file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(file_name);
            format!("{}-{}.{}", stem, project_id, ext)
        }
        None => format!("{}-{}", file_name, project_id),
    }
}
