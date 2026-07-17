//! Pure configuration-extraction helpers for the CrawlFlow pipeline.
//!
//! These functions take a [`PipelineConfig`] (the graph of nodes/edges) and
//! project out the per-stage configuration structs used by the rest of the
//! pipeline (preprocessors, fetch-data nodes, worker definitions, finish
//! actions). They contain no I/O and no orchestration logic — they only read
//! JSON node `data` and build typed values.

use crate::data_preprocessor::{ExtractRule as DpExtractRule, PreprocessorConfig, UrlPattern};
use crate::pipeline::PipelineConfig;
use crate::models::ExtractRule as ModelsExtractRule;
use serde_json::Value;

/// Parse a JSON array of extract rule objects into [`crate::models::ExtractRule`] structs.
///
/// Supports both `{ field, selector, attribute }` (pipeline format) and
/// `{ name, extractFrom, selector, extract, attribute, extractMultiple }`
/// (CrawlFlow UI format).
pub(crate) fn parse_extract_rules_array(arr: &[Value]) -> Vec<ModelsExtractRule> {
    arr.iter()
        .filter_map(|r| {
            // Support both legacy format { field, selector, attribute } and
            // frontend ExtractionRule format { name, extractFrom, selector, extract, attribute, jsonPath }
            let field = r
                .get("field")
                .or_else(|| r.get("name"))
                .and_then(|v| v.as_str())?
                .to_string();

            // Determine the CSS selector or JSON path
            let extract_from = r
                .get("extractFrom")
                .and_then(|v| v.as_str())
                .unwrap_or("html-element");
            let selector = if extract_from == "json-ld" {
                // For JSON-LD, use jsonPath as the "selector" (handled by crawler)
                r.get("jsonPath")
                    .or_else(|| r.get("selector"))
                    .or_else(|| r.get("value"))
                    .and_then(|v| v.as_str())?
                    .to_string()
            } else {
                r.get("selector")
                    .or_else(|| r.get("value"))
                    .and_then(|v| v.as_str())?
                    .to_string()
            };

            // Determine attribute: if extract == 'attribute', use the attribute field
            let extract_mode = r.get("extract").and_then(|v| v.as_str()).unwrap_or("text");
            let attribute = if extract_mode == "attribute" {
                r.get("attribute")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            } else {
                r.get("attribute")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            };

            let extract_multiple = r
                .get("extractMultiple")
                .or_else(|| r.get("extract_multiple"))
                .and_then(|v| v.as_bool());

            Some(ModelsExtractRule {
                field,
                selector,
                attribute,
                extract_multiple,
                extract_from: Some(extract_from.to_string()),
                json_path: r.get("jsonPath").and_then(|v| v.as_str()).map(String::from),
            })
        })
        .collect()
}

/// Extract preprocessor configs from all `preprocessor` nodes in the graph.
pub(crate) fn extract_preprocessors(config: &PipelineConfig) -> Vec<PreprocessorConfig> {
    config
        .nodes
        .iter()
        .filter_map(|node| {
            if node.node_type != "preprocessor" {
                return None;
            }
            let data = &node.data;
            Some(PreprocessorConfig {
                input_type: data
                    .get("inputType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("html")
                    .to_string(),
                item_selector: data
                    .get("itemSelector")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                url_patterns: data
                    .get("urlPatterns")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|p| {
                                Some(UrlPattern {
                                    enabled: p
                                        .get("enabled")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(true),
                                    pattern_type: p
                                        .get("type")
                                        .and_then(|v| v.as_str())?
                                        .to_string(),
                                    value: p.get("value").and_then(|v| v.as_str())?.to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                extract_rules: data
                    .get("extractRules")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|r| {
                                Some(DpExtractRule {
                                    rule_type: r.get("type").and_then(|v| v.as_str())?.to_string(),
                                    value: r.get("value").and_then(|v| v.as_str())?.to_string(),
                                    attribute: r
                                        .get("attribute")
                                        .and_then(|v| v.as_str())
                                        .map(String::from),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                csv_delimiter: data
                    .get("csvDelimiter")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                csv_has_header: data.get("csvHasHeader").and_then(|v| v.as_bool()),
                json_item_path: data
                    .get("jsonItemPath")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                client_type: data
                    .get("clientType")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                client_timeout_secs: data.get("clientTimeoutSecs").and_then(|v| v.as_u64()),
                client_headless: data.get("clientHeadless").and_then(|v| v.as_bool()),
                wait_for_selector: data
                    .get("waitForSelector")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                wait_for_content: data
                    .get("waitForContent")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                wait_timeout_ms: data.get("waitTimeoutMs").and_then(|v| v.as_u64()),
                extract_store_id: data.get("extractStoreId").and_then(|v| v.as_bool()),
                platform: data
                    .get("platform")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
        })
        .collect()
}

/// Extract the `fetchData` node config (URL patterns) as a [`PreprocessorConfig`].
///
/// Returns an empty HTML config if no `fetchData` node is present.
pub(crate) fn extract_fetch_data_config(config: &PipelineConfig) -> PreprocessorConfig {
    for node in &config.nodes {
        if node.node_type == "fetchData" || node.node_type == "fetch_data" {
            let data = &node.data;
            return PreprocessorConfig {
                input_type: "html".into(),
                item_selector: data
                    .get("itemSelector")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                url_patterns: data
                    .get("urlPatterns")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|p| {
                                Some(UrlPattern {
                                    enabled: p
                                        .get("enabled")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(true),
                                    pattern_type: p
                                        .get("type")
                                        .and_then(|v| v.as_str())?
                                        .to_string(),
                                    value: p.get("value").and_then(|v| v.as_str())?.to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                extract_rules: vec![],
                csv_delimiter: None,
                csv_has_header: None,
                json_item_path: None,
                client_type: data
                    .get("clientType")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                client_timeout_secs: data.get("clientTimeoutSecs").and_then(|v| v.as_u64()),
                client_headless: data.get("clientHeadless").and_then(|v| v.as_bool()),
                wait_for_selector: data
                    .get("waitForSelector")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                wait_for_content: data
                    .get("waitForContent")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                wait_timeout_ms: data.get("waitTimeoutMs").and_then(|v| v.as_u64()),
                extract_store_id: Some(false),
                platform: None,
            };
        }
    }
    // No fetchData node found — return empty config
    PreprocessorConfig {
        input_type: "html".into(),
        item_selector: None,
        url_patterns: vec![],
        extract_rules: vec![],
        csv_delimiter: None,
        csv_has_header: None,
        json_item_path: None,
        client_type: None,
        client_timeout_secs: None,
        client_headless: None,
        wait_for_selector: None,
        wait_for_content: None,
        wait_timeout_ms: None,
        extract_store_id: Some(false),
        platform: None,
    }
}

/// Extract a `PaginationConfig` from a node's `data.pagination` field, if present.
pub(crate) fn extract_pagination_config(
    data: &Value,
) -> Option<crate::models::PaginationConfig> {
    let pag_data = data.get("pagination")?;
    Some(serde_json::from_value(pag_data.clone()).ok()?)
}

/// Build the config JSON passed to a Python data-source plugin.
///
/// Injects `shop_url` / `source_url` / `project_id` into the node's
/// `pluginConfig` when not already present.
pub(crate) fn build_plugin_config(
    node_data: &Value,
    source_value: &str,
    project_id: &str,
) -> Value {
    let plugin_config = node_data
        .get("pluginConfig")
        .cloned()
        .unwrap_or(Value::Null);
    let mut config = match &plugin_config {
        Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };
    if !config.contains_key("shop_url") && !source_value.is_empty() {
        config.insert("shop_url".into(), serde_json::json!(source_value));
    }
    config.insert("source_url".into(), serde_json::json!(source_value));
    config.insert("project_id".into(), serde_json::json!(project_id));
    Value::Object(config)
}

/// Helper used by [`crate::pipeline::execute_repository_pipeline`] to build a
/// stable item hash for de-duplication.
pub(crate) fn simple_hash(input: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
