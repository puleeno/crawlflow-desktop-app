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
    pub fn new(builtin_dir: Option<PathBuf>, user_dir: PathBuf) -> Self {
        Self {
            plugins: HashMap::new(),
            python_engine: PythonPluginEngine::new(builtin_dir, user_dir),
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
        let (builtin, user) = self.python_engine.plugin_dirs();
        self.python_engine = PythonPluginEngine::new(builtin.clone(), user.clone());
        self.python_engine.discover()
    }

    // ── Presets ───────────────────────────────────────────────────

    pub fn list_presets(&mut self) -> Vec<serde_json::Value> {
        let mut presets = Vec::new();

        // Built-in presets
        presets.push(serde_json::json!({
            "id": "demo-project",
            "name": "Demo Project",
            "description": "A fully self-contained demo showcasing the CrawlFlow pipeline. No network, no Python — just built-in sample data and processors. Click Run Demo to see results.",
            "icon": "PlayIcon",
            "icon_color": "#22c55e",
            "source": "builtin",
            "plugin_id": null,
            "is_demo": true,
            "project_settings": {
                "name": "CrawlFlow Demo",
                "description": "Self-contained demo — works offline with sample data.",
                "crawlDelay": 0,
                "userAgent": "CrawlFlow/1.0",
                "concurrency": 1,
                "isDemo": "true"
            },
            "nodes": [
                {
                    "id": "ds-1",
                    "type": "start",
                    "label": "Sample Data",
                    "position": {"x": 50, "y": 250},
                    "data": {
                        "sourceType": "url",
                        "sourceValue": "demo://internal/sample",
                        "demoSource": true,
                        "urlSettings": {
                            "scope": "current-url",
                            "excludeExtensions": [],
                            "excludePatterns": [],
                            "whitelistPatterns": [],
                            "domainPolicy": "all",
                            "domainWhitelist": []
                        }
                    }
                },
                {
                    "id": "repo-1",
                    "type": "repository",
                    "label": "Raw Data (5 sample items)",
                    "position": {"x": 250, "y": 250},
                    "data": {}
                },
                {
                    "id": "proc-1",
                    "type": "processor",
                    "label": "Filter (views > 500)",
                    "position": {"x": 450, "y": 180},
                    "data": {
                        "processorType": "rust-filter",
                        "processorConfig": {"field": "views", "operator": "greater_than", "value": "500"}
                    }
                },
                {
                    "id": "proc-2",
                    "type": "processor",
                    "label": "Sort (by views ↓)",
                    "position": {"x": 450, "y": 320},
                    "data": {
                        "processorType": "rust-sort",
                        "processorConfig": {"field": "views", "descending": true}
                    }
                },
                {
                    "id": "proc-3",
                    "type": "processor",
                    "label": "Limit (top 3)",
                    "position": {"x": 650, "y": 250},
                    "data": {
                        "processorType": "rust-limit",
                        "processorConfig": {"count": 3, "offset": 0}
                    }
                },
                {
                    "id": "exp-1",
                    "type": "csvExport",
                    "label": "CSV Export",
                    "position": {"x": 850, "y": 250},
                    "data": {
                        "presets": [],
                        "mappings": [],
                        "hasHeader": true
                    }
                }
            ],
            "edges": [
                {"id": "e-ds-repo", "source": "ds-1", "target": "repo-1", "sourceHandle": "data-out", "targetHandle": "data-in"},
                {"id": "e-repo-filter", "source": "repo-1", "target": "proc-1", "sourceHandle": "data-out", "targetHandle": "data-in"},
                {"id": "e-repo-sort", "source": "repo-1", "target": "proc-2", "sourceHandle": "data-out", "targetHandle": "data-in"},
                {"id": "e-filter-limit", "source": "proc-1", "target": "proc-3", "sourceHandle": "data-out", "targetHandle": "data-in"},
                {"id": "e-sort-limit", "source": "proc-2", "target": "proc-3", "sourceHandle": "data-out", "targetHandle": "data-in"},
                {"id": "e-limit-csv", "source": "proc-3", "target": "exp-1", "sourceHandle": "data-out", "targetHandle": "data-in"}
            ]
        }));

        presets.push(serde_json::json!({
            "id": "web-page-scraper",
            "name": "Web Page Scraper",
            "description": "Fetch a web page, extract content, and export to CSV.",
            "icon": "GlobeAltIcon",
            "icon_color": "#22c55e",
            "source": "builtin",
            "plugin_id": null,
            "project_settings": {
                "name": "Web Scraper - {url}",
                "description": "Scrape web pages for structured data.",
                "crawlDelay": 1000,
                "userAgent": "CrawlFlow/1.0",
                "concurrency": 5
            },
            "nodes": [
                {
                    "id": "ds-1",
                    "type": "start",
                    "label": "From URL",
                    "position": {"x": 50, "y": 200},
                    "data": {
                        "sourceType": "url",
                        "sourceValue": "",
                        "urlSettings": {
                            "scope": "current-url",
                            "excludeExtensions": ["pdf","jpg","png","zip","mp4","svg"],
                            "excludePatterns": [],
                            "whitelistPatterns": [],
                            "domainPolicy": "all",
                            "domainWhitelist": []
                        }
                    }
                },
                {
                    "id": "ext-1",
                    "type": "htmlExtractor",
                    "label": "Extract Data",
                    "position": {"x": 350, "y": 200},
                    "data": {
                        "presets": [],
                        "customRules": []
                    }
                },
                {
                    "id": "exp-1",
                    "type": "csvExport",
                    "label": "CSV Export",
                    "position": {"x": 650, "y": 200},
                    "data": {
                        "presets": [],
                        "mappings": [],
                        "hasHeader": true
                    }
                }
            ],
            "edges": [
                {"id": "e-ds-ext", "source": "ds-1", "target": "ext-1", "sourceHandle": "data-out", "targetHandle": "data-in"},
                {"id": "e-ext-exp", "source": "ext-1", "target": "exp-1", "sourceHandle": "data-out", "targetHandle": "data-in"}
            ]
        }));

        presets.push(serde_json::json!({
            "id": "rss-monitor",
            "name": "RSS Feed Monitor",
            "description": "Monitor an RSS feed, filter items, and save to database.",
            "icon": "RssIcon",
            "icon_color": "#f97316",
            "source": "builtin",
            "plugin_id": null,
            "project_settings": {
                "name": "RSS Monitor - {url}",
                "description": "Monitor RSS feeds for new content.",
                "crawlDelay": 3600000,
                "userAgent": "CrawlFlow/1.0",
                "concurrency": 2
            },
            "nodes": [
                {
                    "id": "ds-1",
                    "type": "rssSource",
                    "label": "RSS Feed",
                    "position": {"x": 50, "y": 200},
                    "data": {
                        "sourceType": "url",
                        "sourceValue": "",
                        "pluginSourceType": "py-rss",
                        "pluginConfig": {}
                    }
                },
                {
                    "id": "proc-1",
                    "type": "processor",
                    "label": "Filter",
                    "position": {"x": 350, "y": 200},
                    "data": {
                        "processorType": "rust-filter",
                        "processorConfig": {
                            "field": "title",
                            "operator": "not_empty",
                            "value": ""
                        }
                    }
                },
                {
                    "id": "exp-1",
                    "type": "databaseExport",
                    "label": "Save to DB",
                    "position": {"x": 650, "y": 200},
                    "data": {
                        "processorType": "save-to-database",
                        "processorConfig": {
                            "strategy": "upsert"
                        }
                    }
                }
            ],
            "edges": [
                {"id": "e-ds-proc", "source": "ds-1", "target": "proc-1", "sourceHandle": "data-out", "targetHandle": "data-in"},
                {"id": "e-proc-exp", "source": "proc-1", "target": "exp-1", "sourceHandle": "data-out", "targetHandle": "data-in"}
            ]
        }));

        presets.push(serde_json::json!({
            "id": "ecommerce-tracker",
            "name": "E-commerce Tracker",
            "description": "Track product prices and availability from e-commerce sites.",
            "icon": "ShoppingCartIcon",
            "icon_color": "#6366f1",
            "source": "builtin",
            "plugin_id": null,
            "project_settings": {
                "name": "Price Tracker - {url}",
                "description": "Track e-commerce product prices.",
                "crawlDelay": 86400000,
                "userAgent": "CrawlFlow/1.0",
                "concurrency": 3
            },
            "nodes": [
                {
                    "id": "ds-1",
                    "type": "start",
                    "label": "Product URL",
                    "position": {"x": 50, "y": 200},
                    "data": {
                        "sourceType": "url",
                        "sourceValue": "",
                        "urlSettings": {
                            "scope": "current-url",
                            "excludeExtensions": [],
                            "excludePatterns": [],
                            "whitelistPatterns": [],
                            "domainPolicy": "all",
                            "domainWhitelist": []
                        }
                    }
                },
                {
                    "id": "ext-1",
                    "type": "htmlExtractor",
                    "label": "Extract Product",
                    "position": {"x": 350, "y": 150},
                    "data": {
                        "presets": ["ecommerce-product"],
                        "customRules": [
                            {"id": "r1", "name": "Title", "extractFrom": "html-element", "selector": "h1", "extract": "text"},
                            {"id": "r2", "name": "Price", "extractFrom": "html-element", "selector": ".price", "extract": "text"},
                            {"id": "r3", "name": "Availability", "extractFrom": "html-element", "selector": ".stock", "extract": "text"},
                            {"id": "r4", "name": "Image", "extractFrom": "html-element", "selector": ".product-image img", "extract": "attribute", "attribute": "src"}
                        ]
                    }
                },
                {
                    "id": "proc-1",
                    "type": "processor",
                    "label": "Deduplicate",
                    "position": {"x": 350, "y": 350},
                    "data": {
                        "processorType": "rust-deduplicate",
                        "processorConfig": {"field": "Title"}
                    }
                },
                {
                    "id": "exp-1",
                    "type": "csvExport",
                    "label": "CSV Export",
                    "position": {"x": 650, "y": 200},
                    "data": {
                        "presets": [],
                        "mappings": [],
                        "hasHeader": true
                    }
                }
            ],
            "edges": [
                {"id": "e-ds-ext", "source": "ds-1", "target": "ext-1", "sourceHandle": "data-out", "targetHandle": "data-in"},
                {"id": "e-ext-proc", "source": "ext-1", "target": "proc-1", "sourceHandle": "data-out", "targetHandle": "data-in"},
                {"id": "e-ext-exp", "source": "proc-1", "target": "exp-1", "sourceHandle": "data-out", "targetHandle": "data-in"}
            ]
        }));

        // Plugin presets
        for p in self.python_engine.collect_presets() {
            presets.push(p);
        }

        presets
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
        let config = serde_json::json!({"field": "title", "operator": "contains", "value": "World"});
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
        assert!(plugins.len() >= 4);
        let ids: Vec<_> = plugins.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"rust-deduplicate"));
        assert!(ids.contains(&"rust-filter"));
        assert!(ids.contains(&"rust-sort"));
        assert!(ids.contains(&"rust-limit"));
    }
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
