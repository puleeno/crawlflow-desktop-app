use crate::item_matcher::{ItemMatcher, MatchPattern};
use crate::models::ClientProfile;
use crate::repository::NewRawItem;
use crate::request_clients;
use serde::{Deserialize, Serialize};

// ── Preprocessor Config ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputType {
    Html,
    Csv,
    Json,
    Xml,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractRule {
    #[serde(rename = "type")]
    pub rule_type: String,
    pub value: String,
    pub attribute: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlPattern {
    pub enabled: bool,
    #[serde(rename = "type")]
    pub pattern_type: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessorConfig {
    pub input_type: String,
    pub item_selector: Option<String>,
    pub url_patterns: Vec<UrlPattern>,
    pub extract_rules: Vec<ExtractRule>,
    pub csv_delimiter: Option<String>,
    pub csv_has_header: Option<bool>,
    pub json_item_path: Option<String>,
    // Allow preprocessor to override client settings for re-fetching
    pub client_type: Option<String>,
    pub client_timeout_secs: Option<u64>,
    pub client_headless: Option<bool>,
    // Wait options for chrome client (AJAX loading)
    pub wait_for_selector: Option<String>,
    pub wait_for_content: Option<String>,
    pub wait_timeout_ms: Option<u64>,
    // Store ID extraction for platforms like oreka
    pub extract_store_id: Option<bool>,
    pub platform: Option<String>,
}

/// Preprocessor registration từ plugin — cho phép plugin đăng ký xử lý riêng
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessorRegistration {
    pub id: String,
    pub name: String,
    pub plugin_id: String,
    pub input_type: String,
    pub platform: Option<String>,
    pub config: PreprocessorConfig,
}

// ── Preprocessor Result ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessorResult {
    pub items: Vec<NewRawItem>,
    pub extracted_count: usize,
    pub errors: Vec<String>,
}

// ── Data Preprocessor Engine ──────────────────────────────

pub struct DataPreprocessor;

impl DataPreprocessor {
    /// Process raw data với plugin dispatch:
    /// 1. Nếu có plugin preprocessor phù hợp → gọi plugin's `preprocess_data`
    /// 2. Fallback vào built-in xử lý
    pub fn process_with_plugins(
        raw_data: &str,
        source_url: &str,
        config: &PreprocessorConfig,
        python_engine: &mut crate::python_plugins::PythonPluginEngine,
    ) -> PreprocessorResult {
        // Tìm plugin preprocessor phù hợp dựa trên platform / input_type
        let registrations = python_engine.collect_preprocessors();
        let matched = registrations.iter().find(|r| {
            let platform_matches = match &r.platform {
                Some(p) => source_url.contains(p),
                None => true,
            };
            platform_matches && r.input_type == config.input_type
        });

        if let Some(reg) = matched {
            // Plugin tồn tại — thử gọi preprocess_data hook
            let data_json = serde_json::json!({
                "raw_data": raw_data,
                "source_url": source_url,
                "config": config,
            });
            let result = python_engine.call_preprocessor_hook(&reg.plugin_id, data_json);
            match result {
                Ok(items) => {
                    if items.is_empty() {
                        log::warn!(
                            "Plugin preprocessor '{}' returned no items (fallback to built-in)",
                            reg.plugin_id
                        );
                        return Self::process(raw_data, source_url, config);
                    }

                    let count = items.len();
                    return PreprocessorResult {
                        items,
                        extracted_count: count,
                        errors: vec![],
                    };
                }
                Err(e) => {
                    // Plugin hook thất bại → fallback vào built-in
                    log::warn!(
                        "Plugin preprocessor '{}' failed (fallback to built-in): {}",
                        reg.plugin_id,
                        e
                    );
                }
            }
        }

        // Fallback: built-in xử lý
        Self::process(raw_data, source_url, config)
    }

    /// Process with async support for re-fetching (used in pipeline)
    pub async fn process_async(
        raw_data: &str,
        source_url: &str,
        config: &PreprocessorConfig,
    ) -> PreprocessorResult {
        let auto_extract_store_id = Self::should_auto_extract_store_id(source_url, config);
        let extract_store_id = config.extract_store_id.unwrap_or(auto_extract_store_id);
        if auto_extract_store_id && config.extract_store_id.is_none() {
            log::info!(
                "[preprocessing] Inferred store-ID rewrite flow from Oreka store URL: {}",
                source_url
            );
        }
        let resolved_html = if extract_store_id
            && (raw_data.trim().is_empty() || !raw_data.trim_start().starts_with('<'))
        {
            log::info!(
                "[preprocessing] Store ID extraction enabled, source HTML missing so refetching before rewrite: {}",
                source_url
            );
            Self::refetch_with_client(source_url, config)
                .unwrap_or_else(|| raw_data.to_string())
        } else {
            raw_data.to_string()
        };

        if extract_store_id {
            return Self::extract_store_id_only(&resolved_html, source_url, config);
        }

        // If preprocessor has custom client settings, re-fetch with that client
        if config.client_type.is_some()
            || config.client_timeout_secs.is_some()
            || config.wait_for_selector.is_some()
            || config.wait_for_content.is_some()
        {
            let profile = ClientProfile {
                client_type: config
                    .client_type
                    .clone()
                    .unwrap_or_else(|| "reqwest".to_string()),
                timeout_secs: config.client_timeout_secs,
                headless: config.client_headless,
                ..Default::default()
            };

            let result = request_clients::fetch_with_client(
                source_url,
                &profile,
                None,
                config.wait_for_selector.as_deref(),
                config.wait_for_content.as_deref(),
                config.wait_timeout_ms,
            )
            .await;
            if let Some(refreshed_data) = result.html {
                return Self::process_internal(&refreshed_data, source_url, config);
            }
        }

        Self::process_internal(&resolved_html, source_url, config)
    }

    /// Process raw data từ data source theo config, trích xuất items
    pub fn process(
        raw_data: &str,
        source_url: &str,
        config: &PreprocessorConfig,
    ) -> PreprocessorResult {
        let auto_extract_store_id = Self::should_auto_extract_store_id(source_url, config);
        let extract_store_id = config.extract_store_id.unwrap_or(auto_extract_store_id);
        if auto_extract_store_id && config.extract_store_id.is_none() {
            log::info!(
                "[preprocessing] Inferred store-ID rewrite flow from Oreka store URL: {}",
                source_url
            );
        }
        let resolved_html = if extract_store_id
            && (raw_data.trim().is_empty() || !raw_data.trim_start().starts_with('<'))
        {
            log::info!(
                "[preprocessing] Store ID extraction enabled, source HTML missing so refetching before rewrite: {}",
                source_url
            );
            Self::refetch_with_client(source_url, config)
                .unwrap_or_else(|| raw_data.to_string())
        } else {
            raw_data.to_string()
        };

        // If extract_store_id is enabled, focus on store ID extraction only
        if extract_store_id {
            return Self::extract_store_id_only(&resolved_html, source_url, config);
        }

        // If preprocessor has custom client settings, re-fetch with that client
        if config.client_type.is_some()
            || config.client_timeout_secs.is_some()
            || config.wait_for_selector.is_some()
            || config.wait_for_content.is_some()
        {
            if let Some(refreshed_data) = Self::refetch_with_client(source_url, config) {
                return Self::process_internal(&refreshed_data, source_url, config);
            }
        }

        Self::process_internal(&resolved_html, source_url, config)
    }

    fn should_auto_extract_store_id(source_url: &str, config: &PreprocessorConfig) -> bool {
        let platform = config.platform.as_deref().unwrap_or("").to_ascii_lowercase();
        let source_is_oreka_store = source_url.contains("oreka.vn/store/")
            || source_url.contains("oreka.vn/mua-ban?");
        source_is_oreka_store && (platform.contains("oreka") || source_url.contains("oreka.vn"))
    }

    /// Extract store ID from HTML for platforms like oreka.vn
    /// If store ID is found, rewrite the source URL into a store-listing URL
    /// and try to extract the actual child product URLs from that fetched page.
    fn extract_store_id_only(
        raw_data: &str,
        source_url: &str,
        config: &PreprocessorConfig,
    ) -> PreprocessorResult {
        let platform = config.platform.as_deref().unwrap_or("oreka.vn");

        log::info!(
            "[preprocessing] Attempting to resolve storeId from HTML for platform={} source_url={}",
            platform,
            source_url
        );

        let store_id = Self::extract_store_id_from_html(raw_data, platform);

        if let Some(store_id) = store_id {
            log::info!(
                "[preprocessing] Resolved storeId={} from HTML for source_url={}",
                store_id,
                source_url
            );

            let transformed_url = Self::build_store_url(source_url, &store_id, platform);
            log::info!(
                "[preprocessing] Rewrote store URL to listing format: {}",
                transformed_url
            );

            let listing_html = Self::refetch_with_client(&transformed_url, config)
                .unwrap_or_else(|| raw_data.to_string());
            log::info!(
                "[preprocessing] Refetched listing page for rewritten URL ({} bytes)",
                listing_html.len()
            );

            let listing_result = Self::process_internal(&listing_html, &transformed_url, config);
            if listing_result.extracted_count > 0 {
                log::info!(
                    "[preprocessing] Extracted {} concrete product URLs from rewritten listing page",
                    listing_result.extracted_count
                );
                return listing_result;
            }

            log::warn!(
                "[preprocessing] No concrete product URLs were extracted from rewritten listing page; falling back to a single rewritten URL item"
            );
            let item = NewRawItem {
                source_url: transformed_url.clone(),
                item_type: "url".to_string(),
                item_hash: format!("{:x}", md5::compute(transformed_url.as_bytes())),
                raw_content: Some(listing_html),
                extracted_url: Some(transformed_url),
            };

            PreprocessorResult {
                items: vec![item],
                extracted_count: 1,
                errors: vec![],
            }
        } else {
            log::warn!(
                "[preprocessing] Could not extract storeId from HTML for platform={} source_url={}",
                platform,
                source_url
            );
            PreprocessorResult {
                items: vec![],
                extracted_count: 0,
                errors: vec![format!("Could not extract store ID from {}", platform)],
            }
        }
    }

    fn extract_store_id_from_html(html: &str, platform: &str) -> Option<String> {
        match platform {
            "oreka.vn" => {
                // Try to extract store ID from various patterns
                // Pattern 1: __NEXT_DATA__ Apollo cache
                if let Some(next_data) = Self::extract_next_data(html) {
                    if let Some(store_id) = Self::find_apollo_store_id(&next_data) {
                        return Some(store_id);
                    }
                }
                
                // Pattern 2: Direct UUID pattern
                let uuid_pattern = regex::Regex::new(r#"Store:([a-fA-F0-9\-]{36})"#).ok()?;
                if let Some(caps) = uuid_pattern.captures(html) {
                    return Some(caps.get(1)?.as_str().to_string());
                }
                
                // Pattern 3: storeId in JSON
                let store_id_pattern = regex::Regex::new(r#""storeId"\s*:\s*"([^"]+)""#).ok()?;
                if let Some(caps) = store_id_pattern.captures(html) {
                    return Some(caps.get(1)?.as_str().to_string());
                }
                
                None
            }
            _ => None,
        }
    }

    fn extract_next_data(html: &str) -> Option<serde_json::Value> {
        let pattern = regex::Regex::new(r#"<script\b[^>]*id="__NEXT_DATA__"[^>]*>(.*?)</script>"#).ok()?;
        if let Some(caps) = pattern.captures(html) {
            let json_str = caps.get(1)?.as_str();
            serde_json::from_str(json_str).ok()
        } else {
            None
        }
    }

    fn find_apollo_store_id(data: &serde_json::Value) -> Option<String> {
        // Pattern 1: props.pageProps.__APOLLO_STATE__ (actual Next.js structure from Oreka)
        if let Some(apollo_state) = data
            .get("props")
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("pageProps"))
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("__APOLLO_STATE__"))
            .and_then(|v| v.as_object())
        {
            for (key, value) in apollo_state {
                if key.starts_with("Store:") {
                    if let Some(store_id) = value.get("id").and_then(|v| v.as_str()) {
                        return Some(store_id.to_string());
                    }
                    // Use key as fallback
                    if let Some(store_id) = key.strip_prefix("Store:") {
                        return Some(store_id.to_string());
                    }
                }
            }
        }

        // Pattern 2: props.pageProps.dehydratedState.queries[].state.data.storeProfile.storeId (test case)
        if let Some(dehydrated) = data
            .get("props")
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("pageProps"))
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("dehydratedState"))
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get("queries"))
            .and_then(|v| v.as_array())
        {
            for query in dehydrated {
                if let Some(store_id) = query
                    .get("state")
                    .and_then(|v| v.as_object())
                    .and_then(|obj| obj.get("data"))
                    .and_then(|v| v.as_object())
                    .and_then(|obj| obj.get("storeProfile"))
                    .and_then(|v| v.as_object())
                    .and_then(|obj| obj.get("storeId"))
                    .and_then(|v| v.as_str())
                {
                    return Some(store_id.to_string());
                }
            }
        }

        None
    }

    fn build_store_url(source_url: &str, store_id: &str, platform: &str) -> String {
        match platform {
            "oreka.vn" => {
                let parsed = url::Url::parse(source_url)
                    .unwrap_or_else(|_| url::Url::parse("https://www.oreka.vn").unwrap());
                let host = parsed.host_str().unwrap_or("www.oreka.vn");
                let base = format!("{}://{}", parsed.scheme(), host);
                format!("{}/mua-ban?storeId={}&sort=createdAt&order=desc", base, store_id)
            }
            _ => source_url.to_string(),
        }
    }

    /// Re-fetch URL with custom client settings from preprocessor config
    fn refetch_with_client(source_url: &str, config: &PreprocessorConfig) -> Option<String> {
        let rt = tokio::runtime::Runtime::new().ok()?;
        let profile = ClientProfile {
            client_type: config
                .client_type
                .clone()
                .unwrap_or_else(|| "reqwest".to_string()),
            timeout_secs: config.client_timeout_secs,
            headless: config.client_headless,
            ..Default::default()
        };

        let result = rt.block_on(request_clients::fetch_with_client(
            source_url,
            &profile,
            None,
            config.wait_for_selector.as_deref(),
            config.wait_for_content.as_deref(),
            config.wait_timeout_ms,
        ));
        result.html
    }

    /// Internal processing logic (used after potential re-fetch)
    fn process_internal(
        raw_data: &str,
        source_url: &str,
        config: &PreprocessorConfig,
    ) -> PreprocessorResult {
        match config.input_type.as_str() {
            "html" => Self::process_html(raw_data, source_url, config),
            "csv" => Self::process_csv(raw_data, source_url, config),
            "json" => Self::process_json(raw_data, source_url, config),
            "xml" => Self::process_xml(raw_data, source_url, config),
            _ => Self::process_text(raw_data, source_url, config),
        }
    }

    // ── HTML Processing ───────────────────────────────────────

    fn process_html(
        html: &str,
        source_url: &str,
        config: &PreprocessorConfig,
    ) -> PreprocessorResult {
        let mut items = Vec::new();
        let errors = Vec::new();

        // Convert URL patterns to MatchPattern
        let match_patterns: Vec<MatchPattern> = config
            .url_patterns
            .iter()
            .filter(|p| p.enabled)
            .filter_map(|p| match p.pattern_type.as_str() {
                "wildcard" => Some(MatchPattern::Wildcard(p.value.clone())),
                "regex" => Some(MatchPattern::Regex(p.value.clone())),
                "contains" => Some(MatchPattern::Contains(p.value.clone())),
                "startswith" => Some(MatchPattern::StartsWith(p.value.clone())),
                "endswith" => Some(MatchPattern::EndsWith(p.value.clone())),
                "always" | "all" => Some(MatchPattern::Always),
                _ => None,
            })
            .collect();

        if let Some(selector) = &config.item_selector {
            // Extract items by CSS selector → then extract URLs from each item
            let items_html = Self::extract_by_selector(html, selector);
            for item_html in items_html {
                let urls =
                    ItemMatcher::extract_matching_urls(&item_html, source_url, &match_patterns);
                for url in urls {
                    let item_hash = Self::hash(&url);
                    items.push(NewRawItem {
                        source_url: source_url.to_string(),
                        item_type: "url".into(),
                        item_hash,
                        raw_content: Some(item_html.clone()),
                        extracted_url: Some(url),
                    });
                }
            }
        } else {
            // No CSS selector → extract URLs directly from full HTML
            let urls = ItemMatcher::extract_matching_urls(html, source_url, &match_patterns);
            if urls.is_empty() {
                // Fallback: save entire HTML as one item
                let item_hash = Self::hash(html);
                items.push(NewRawItem {
                    source_url: source_url.to_string(),
                    item_type: "page".into(),
                    item_hash,
                    raw_content: Some(html.to_string()),
                    extracted_url: None,
                });
            } else {
                for url in urls {
                    let item_hash = Self::hash(&url);
                    items.push(NewRawItem {
                        source_url: source_url.to_string(),
                        item_type: "url".into(),
                        item_hash,
                        raw_content: None,
                        extracted_url: Some(url),
                    });
                }
            }
        }

        // Apply extract rules (field extraction từ HTML)
        if !config.extract_rules.is_empty() {
            for item in &mut items {
                if let Some(content) = &item.raw_content {
                    let extracted = Self::apply_extract_rules(content, &config.extract_rules);
                    item.raw_content = Some(extracted);
                }
            }
        }

        PreprocessorResult {
            extracted_count: items.len(),
            items,
            errors,
        }
    }

    // ── CSV Processing ────────────────────────────────────────

    fn process_csv(
        data: &str,
        source_url: &str,
        config: &PreprocessorConfig,
    ) -> PreprocessorResult {
        let mut items = Vec::new();
        let errors = Vec::new();
        let delimiter = config.csv_delimiter.as_deref().unwrap_or(",");
        let has_header = config.csv_has_header.unwrap_or(true);

        let mut lines = data.lines().filter(|l| !l.trim().is_empty());
        let headers: Vec<String> = if has_header {
            lines
                .next()
                .map(|line| {
                    line.split(delimiter)
                        .map(|s| s.trim().to_string())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            (0..20).map(|i| format!("col_{}", i)).collect()
        };

        for line in lines {
            let cols: Vec<&str> = line.split(delimiter).collect();
            let mut fields = serde_json::Map::new();
            for (i, col) in cols.iter().enumerate() {
                let key = headers
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("col_{}", i));
                fields.insert(key, serde_json::Value::String(col.trim().to_string()));
            }

            let row_json = serde_json::Value::Object(fields);
            let row_str = row_json.to_string();
            let item_hash = Self::hash(&row_str);

            items.push(NewRawItem {
                source_url: source_url.to_string(),
                item_type: "csv_row".into(),
                item_hash,
                raw_content: Some(row_str),
                extracted_url: None,
            });
        }

        PreprocessorResult {
            extracted_count: items.len(),
            items,
            errors,
        }
    }

    // ── JSON Processing ───────────────────────────────────────

    fn process_json(
        data: &str,
        source_url: &str,
        config: &PreprocessorConfig,
    ) -> PreprocessorResult {
        let mut items = Vec::new();
        let mut errors = Vec::new();

        // Parse JSON
        let json: serde_json::Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(e) => {
                errors.push(format!("Invalid JSON: {}", e));
                return PreprocessorResult {
                    extracted_count: 0,
                    items,
                    errors,
                };
            }
        };

        // Navigate to item array using json path
        let item_array = if let Some(path) = &config.json_item_path {
            Self::navigate_json_path(&json, path)
        } else {
            json.clone()
        };

        let arr = match &item_array {
            serde_json::Value::Array(a) => a.clone(),
            _ => vec![item_array],
        };

        for item_val in arr {
            let item_str = item_val.to_string();
            let item_hash = Self::hash(&item_str);
            let extracted_url = item_val
                .get("url")
                .or_else(|| item_val.get("link"))
                .or_else(|| item_val.get("href"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            items.push(NewRawItem {
                source_url: source_url.to_string(),
                item_type: "json_item".into(),
                item_hash,
                raw_content: Some(item_str),
                extracted_url,
            });
        }

        PreprocessorResult {
            extracted_count: items.len(),
            items,
            errors,
        }
    }

    // ── XML Processing ────────────────────────────────────────

    fn process_xml(
        data: &str,
        source_url: &str,
        config: &PreprocessorConfig,
    ) -> PreprocessorResult {
        let mut items = Vec::new();
        let mut errors = Vec::new();

        // Extract item-like tags using regex (simple XML parsing)
        let item_tag = config.item_selector.as_deref().unwrap_or("item");
        let re = match regex::Regex::new(&format!(
            r"<{}[^>]*>(.*?)</{}>",
            regex::escape(item_tag),
            regex::escape(item_tag)
        )) {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("Invalid XML pattern: {}", e));
                return PreprocessorResult {
                    extracted_count: 0,
                    items,
                    errors,
                };
            }
        };

        for cap in re.captures_iter(data) {
            let content = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            let item_hash = Self::hash(&content);

            // Extract URL from <link> or <url> child
            let extracted_url = regex::Regex::new(r"<(?:link|url)>(.*?)</(?:link|url)>")
                .ok()
                .and_then(|re| re.captures(&content))
                .map(|c| c[1].to_string());

            items.push(NewRawItem {
                source_url: source_url.to_string(),
                item_type: "xml_item".into(),
                item_hash,
                raw_content: Some(content),
                extracted_url,
            });
        }

        if items.is_empty() {
            // Fallback: save whole XML
            let item_hash = Self::hash(data);
            items.push(NewRawItem {
                source_url: source_url.to_string(),
                item_type: "xml_doc".into(),
                item_hash,
                raw_content: Some(data.to_string()),
                extracted_url: None,
            });
        }

        PreprocessorResult {
            extracted_count: items.len(),
            items,
            errors,
        }
    }

    // ── Text Processing ───────────────────────────────────────

    fn process_text(
        data: &str,
        source_url: &str,
        _config: &PreprocessorConfig,
    ) -> PreprocessorResult {
        let mut items = Vec::new();

        // Mỗi dòng là 1 item
        for line in data.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let item_hash = Self::hash(line);
            let is_url = line.starts_with("http://") || line.starts_with("https://");

            items.push(NewRawItem {
                source_url: source_url.to_string(),
                item_type: if is_url { "url".into() } else { "text".into() },
                item_hash,
                raw_content: Some(line.to_string()),
                extracted_url: if is_url { Some(line.to_string()) } else { None },
            });
        }

        PreprocessorResult {
            extracted_count: items.len(),
            items,
            errors: vec![],
        }
    }

    // ── Helpers ───────────────────────────────────────────────

    /// Simple CSS selector → extract outer HTML (full element including tags)
    fn extract_by_selector(html: &str, selector: &str) -> Vec<String> {
        use scraper::{Html, Selector};

        let document = Html::parse_fragment(html);
        if let Ok(sel) = Selector::parse(selector) {
            document.select(&sel).map(|el| el.html()).collect()
        } else {
            vec![]
        }
    }

    /// Apply field extraction rules to HTML content
    fn apply_extract_rules(html: &str, rules: &[ExtractRule]) -> String {
        use scraper::{Html, Selector};

        let mut result = serde_json::Map::new();
        let document = Html::parse_fragment(html);

        for rule in rules {
            if let Ok(sel) = Selector::parse(&rule.value) {
                if let Some(element) = document.select(&sel).next() {
                    let text = if let Some(attr) = &rule.attribute {
                        element.value().attr(attr).unwrap_or("").to_string()
                    } else {
                        element
                            .text()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .trim()
                            .to_string()
                    };
                    result.insert(rule.rule_type.clone(), serde_json::Value::String(text));
                }
            }
        }

        if result.is_empty() {
            html.to_string()
        } else {
            serde_json::to_string(&result).unwrap_or_else(|_| html.to_string())
        }
    }

    /// Simple JSON path navigation (e.g., "data.items")
    fn navigate_json_path(json: &serde_json::Value, path: &str) -> serde_json::Value {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json.clone();
        for part in parts {
            match current {
                serde_json::Value::Object(ref m) => {
                    current = m.get(part).cloned().unwrap_or(serde_json::Value::Null);
                }
                serde_json::Value::Array(ref a) => {
                    if let Ok(idx) = part.parse::<usize>() {
                        current = a.get(idx).cloned().unwrap_or(serde_json::Value::Null);
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
        current
    }

    fn hash(input: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        input.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_processing() {
        let csv = "title,price\nProduct A,100\nProduct B,200\n";
        let config = PreprocessorConfig {
            input_type: "csv".into(),
            item_selector: None,
            url_patterns: vec![],
            extract_rules: vec![],
            csv_delimiter: Some(",".into()),
            csv_has_header: Some(true),
            json_item_path: None,
            client_type: None,
            client_timeout_secs: None,
            client_headless: None,
            wait_for_selector: None,
            wait_for_content: None,
            wait_timeout_ms: None,
            extract_store_id: None,
            platform: None,
        };
        let result = DataPreprocessor::process(csv, "https://example.com/data.csv", &config);
        assert_eq!(result.extracted_count, 2);
    }

    #[test]
    fn test_json_processing() {
        let json = r#"{"items": [{"title": "A", "url": "https://a.com"}, {"title": "B", "url": "https://b.com"}]}"#;
        let config = PreprocessorConfig {
            input_type: "json".into(),
            item_selector: None,
            url_patterns: vec![],
            extract_rules: vec![],
            csv_delimiter: None,
            csv_has_header: None,
            json_item_path: Some("items".into()),
            client_type: None,
            client_timeout_secs: None,
            client_headless: None,
            wait_for_selector: None,
            wait_for_content: None,
            wait_timeout_ms: None,
            extract_store_id: None,
            platform: None,
        };
        let result = DataPreprocessor::process(json, "https://example.com/data.json", &config);
        assert_eq!(result.extracted_count, 2);
        assert_eq!(
            result.items[0].extracted_url.as_deref(),
            Some("https://a.com")
        );
    }

    #[test]
    fn test_html_url_extraction() {
        let html =
            r#"<a href="/product/1">P1</a><a href="/product/2">P2</a><a href="/blog">Blog</a>"#;
        let config = PreprocessorConfig {
            input_type: "html".into(),
            item_selector: None,
            url_patterns: vec![UrlPattern {
                enabled: true,
                pattern_type: "contains".into(),
                value: "/product/".into(),
            }],
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
            extract_store_id: None,
            platform: None,
        };
        let result = DataPreprocessor::process(html, "https://example.com", &config);
        assert_eq!(result.extracted_count, 2);
    }

    #[test]
    fn test_html_url_extraction_with_oreka_regex() {
        let html = r#"<a href="/mua-ban-dong-ho/moi--detail/1112311">Đồng hồ mới</a>
                      <a href="https://www.oreka.vn/mua-ban-sach/sach-hay--detail/2223422">Sách hay</a>
                      <a href="/mua-ban-dien-tu/laptop--detail/3334533">Laptop</a>
                      <a href="/blog/post">Blog</a>
                      <a href="/store/C21AVGZS44L3UU">Cửa hàng</a>"#;
        let config = PreprocessorConfig {
            input_type: "html".into(),
            item_selector: None,
            url_patterns: vec![UrlPattern {
                enabled: true,
                pattern_type: "regex".into(),
                value: ".*-detail\\/[0-9]{1,}\\/?".into(),
            }],
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
            extract_store_id: None,
            platform: None,
        };
        let result = DataPreprocessor::process(html, "https://www.oreka.vn", &config);
        assert_eq!(result.extracted_count, 3);
        assert!(result.items[0].extracted_url.as_deref().unwrap().contains("--detail/1112311"));
        assert!(result.items[1].extracted_url.as_deref().unwrap().contains("--detail/2223422"));
        assert!(result.items[2].extracted_url.as_deref().unwrap().contains("--detail/3334533"));
    }

#[test]
    fn test_html_url_extraction_with_oreka_store_page() {
        // Test case 1: dehydratedState pattern
        let html1 = r#"<!DOCTYPE html>
<html>
<head><script id="__NEXT_DATA__" type="application/json">
{"props":{"pageProps":{"dehydratedState":{"queries":[{"state":{"data":{"storeProfile":{"storeId":"C21AVGZS44L3UU","storeName":"Mộc Bản","sellingCount":10,"soldCount":2}}}}]}}}}}
</script></head>
<body>
<div class="shop-header">Mộc Bản</div>
<div class="product-listing">
  <p>Không có kết quả</p>
  <span>Xem tất cả (0)</span>
</div>
</body>
</html>"#;
        let config1 = PreprocessorConfig {
            input_type: "html".into(),
            item_selector: None,
            url_patterns: vec![UrlPattern {
                enabled: true,
                pattern_type: "regex".into(),
                value: ".*-detail\\/[0-9]{1,}\\/?".into(),
            }],
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
            extract_store_id: Some(true),
            platform: Some("oreka.vn".into()),
        };
        let result1 = DataPreprocessor::process(html1, "https://www.oreka.vn/store/C21AVGZS44L3UU", &config1);
        assert_eq!(result1.extracted_count, 1);
        assert!(result1.items[0].extracted_url.as_deref().unwrap().contains("storeId=C21AVGZS44L3UU"));

        // Test case 2: __APOLLO_STATE__ pattern (actual Oreka site structure)
        let html2 = r#"<!DOCTYPE html>
<html>
<head><script id="__NEXT_DATA__" type="application/json">
{"props":{"pageProps":{"__APOLLO_STATE__":{"Store:a15d6cec-1b05-4307-b90c-0afb9552fb5e":{"__typename":"Store","id":"a15d6cec-1b05-4307-b90c-0afb9552fb5e","slug":"muabansachcuvn","name":"Muabansachcu.vn"}}}}}}
</script></head>
<body>
<div class="shop-header">Muabansachcu.vn</div>
<div class="product-listing">
  <p>Không có kết quả</p>
  <span>Xem tất cả (0)</span>
</div>
</body>
</html>"#;
        let config2 = PreprocessorConfig {
            input_type: "html".into(),
            item_selector: None,
            url_patterns: vec![UrlPattern {
                enabled: true,
                pattern_type: "regex".into(),
                value: ".*-detail\\/[0-9]{1,}\\/?".into(),
            }],
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
            extract_store_id: Some(true),
            platform: Some("oreka.vn".into()),
        };
        let result2 = DataPreprocessor::process(html2, "https://www.oreka.vn/store/muabansachcuvn", &config2);
        assert_eq!(result2.extracted_count, 1);
        assert!(result2.items[0].extracted_url.as_deref().unwrap().contains("storeId=a15d6cec-1b05-4307-b90c-0afb9552fb5e"));
    }

    #[test]
    fn test_oreka_store_url_infers_store_id_rewrite_even_without_explicit_flags() {
        let html = r#"<!DOCTYPE html>
<html>
<head><script id="__NEXT_DATA__" type="application/json">
{"props":{"pageProps":{"dehydratedState":{"queries":[{"state":{"data":{"storeProfile":{"storeId":"C21AVGZS44L3UU","storeName":"Mộc Bản","sellingCount":10,"soldCount":2}}}}]}}}}}
</script></head>
<body>
<div class="shop-header">Mộc Bản</div>
</body>
</html>"#;

        let config = PreprocessorConfig {
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
            extract_store_id: None,
            platform: None,
        };

        let should_extract = DataPreprocessor::should_auto_extract_store_id(
            "https://www.oreka.vn/store/C21AVGZS44L3UU",
            &config,
        );
        assert!(should_extract);

        let store_id = DataPreprocessor::extract_store_id_from_html(html, "oreka.vn");
        assert_eq!(store_id.as_deref(), Some("C21AVGZS44L3UU"));

        let rewritten = DataPreprocessor::build_store_url(
            "https://www.oreka.vn/store/C21AVGZS44L3UU",
            "C21AVGZS44L3UU",
            "oreka.vn",
        );
        assert!(rewritten.contains("storeId=C21AVGZS44L3UU"));
    }
}
