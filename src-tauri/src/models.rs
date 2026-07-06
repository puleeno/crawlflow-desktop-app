use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_profile_default() {
        let p = ClientProfile::default();
        assert_eq!(p.client_type, "reqwest");
        assert_eq!(p.user_agent, Some("CrawlFlow/1.0".into()));
        assert_eq!(p.timeout_secs, Some(30));
        assert!(p.proxy_url.is_none());
        assert!(p.headers.is_none());
    }

    #[test]
    fn test_client_profile_serde_roundtrip() {
        let p = ClientProfile {
            client_type: "chrome".into(),
            user_agent: Some("TestBot/1.0".into()),
            proxy_url: Some("http://proxy:8080".into()),
            headers: Some(vec![("X-Custom".into(), "val".into())]),
            timeout_secs: Some(60),
            profile_dir: Some("/tmp/profiles".into()),
            chrome_args: Some(vec!["--no-sandbox".into()]),
            wait_for_selector: Some(".loaded".into()),
            extra_nav_args: Some(vec!["--flag".into()]),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: ClientProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.client_type, "chrome");
        assert_eq!(back.user_agent, Some("TestBot/1.0".into()));
        assert_eq!(back.proxy_url, Some("http://proxy:8080".into()));
        assert_eq!(back.timeout_secs, Some(60));
        assert_eq!(back.profile_dir, Some("/tmp/profiles".into()));
        assert_eq!(back.wait_for_selector, Some(".loaded".into()));
    }

    #[test]
    fn test_crawl_request_serde_roundtrip() {
        let req = CrawlRequest {
            url: "https://example.com".into(),
            method: Some("GET".into()),
            headers: Some(vec![HeaderPair {
                key: "Accept".into(),
                value: "text/html".into(),
            }]),
            body: None,
            use_browser: Some(false),
            wait_for_selector: None,
            extract_rules: Some(vec![ExtractRule {
                field: "title".into(),
                selector: "h1".into(),
                attribute: None,
                extract_multiple: None,
            }]),
            client_profile: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CrawlRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.url, "https://example.com");
        assert_eq!(back.method, Some("GET".into()));
        assert!(back.client_profile.is_none());
        assert_eq!(back.extract_rules.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_crawl_result_serde_roundtrip() {
        let result = CrawlResult {
            url: "https://example.com".into(),
            status: 200,
            html: Some("<h1>OK</h1>".into()),
            text: Some("OK".into()),
            extracted: Some(vec![ExtractedField {
                field: "title".into(),
                values: vec!["OK".into()],
            }]),
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: CrawlResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, 200);
        assert_eq!(back.text, Some("OK".into()));
        assert!(back.error.is_none());
    }

    #[test]
    fn test_crawl_result_with_error() {
        let result = CrawlResult {
            url: "https://example.com".into(),
            status: 0,
            html: None,
            text: None,
            extracted: None,
            error: Some("Connection refused".into()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: CrawlResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.status, 0);
        assert_eq!(back.error, Some("Connection refused".into()));
        assert!(back.html.is_none());
    }

    #[test]
    fn test_extract_rule_defaults() {
        let rule: ExtractRule =
            serde_json::from_str(r#"{"field":"title","selector":"h1"}"#).unwrap();
        assert_eq!(rule.field, "title");
        assert_eq!(rule.selector, "h1");
        assert!(rule.attribute.is_none());
        assert!(rule.extract_multiple.is_none());
    }

    #[test]
    fn test_plugin_info_serde() {
        let info = PluginInfo {
            id: "test-plugin".into(),
            name: "Test".into(),
            version: "1.0.0".into(),
            description: "A test".into(),
            capabilities: vec!["processor".into(), "export".into()],
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: PluginInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "test-plugin");
        assert_eq!(back.capabilities.len(), 2);
    }

    #[test]
    fn test_parsed_html_item_defaults() {
        let json = r#"{"tag":"h1","text":"Title","html":"<h1>Title</h1>","type":"heading","attributes":{}}"#;
        let item: ParsedHtmlItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.tag, "h1");
        assert_eq!(item.item_type, "heading");
        assert!(item.href.is_empty());
        assert!(item.src.is_empty());
        assert_eq!(item.table_index, 0);
    }

    #[test]
    fn test_parsed_html_item_with_table() {
        let json = r#"{"tag":"table","text":"","html":"<table>...</table>","type":"table","attributes":{},"table_index":1,"table_data":[["a","b"],["c","d"]]}"#;
        let item: ParsedHtmlItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.table_index, 1);
        assert_eq!(item.table_data.len(), 2);
        assert_eq!(item.table_data[0][0], "a");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientProfile {
    pub client_type: String,
    pub user_agent: Option<String>,
    pub proxy_url: Option<String>,
    pub headers: Option<Vec<(String, String)>>,
    pub timeout_secs: Option<u64>,
    pub profile_dir: Option<String>,
    pub chrome_args: Option<Vec<String>>,
    pub wait_for_selector: Option<String>,
    pub extra_nav_args: Option<Vec<String>>,
}

impl Default for ClientProfile {
    fn default() -> Self {
        Self {
            client_type: "reqwest".into(),
            user_agent: Some("CrawlFlow/1.0".into()),
            proxy_url: None,
            headers: None,
            timeout_secs: Some(30),
            profile_dir: None,
            chrome_args: None,
            wait_for_selector: None,
            extra_nav_args: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlRequest {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<Vec<HeaderPair>>,
    pub body: Option<String>,
    pub use_browser: Option<bool>,
    pub wait_for_selector: Option<String>,
    pub extract_rules: Option<Vec<ExtractRule>>,
    pub client_profile: Option<ClientProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderPair {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractRule {
    #[serde(alias = "name")]
    pub field: String,
    pub selector: String,
    pub attribute: Option<String>,
    #[serde(alias = "extractMultiple")]
    pub extract_multiple: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlResult {
    pub url: String,
    pub status: u16,
    pub html: Option<String>,
    pub text: Option<String>,
    pub extracted: Option<Vec<ExtractedField>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedField {
    pub field: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRequest {
    pub processor_type: String,
    pub data: Vec<serde_json::Value>,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessResult {
    pub success: bool,
    pub data: Vec<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseRequest {
    pub parser_id: String,
    pub input: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub icon_color: String,
    pub source: String,
    pub plugin_id: Option<String>,
    pub project_settings: serde_json::Value,
    pub nodes: serde_json::Value,
    pub edges: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    pub format: String,
    pub data: Vec<serde_json::Value>,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub file_name: String,
    pub mime_type: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssFetchRequest {
    pub feed_url: String,
    pub max_items: Option<usize>,
}

// ── BeautifulSoup Parsed HTML (Python → Rust) ─────────────────

/// A single parsed HTML element, returned by the BeautifulSoup Python plugin.
/// Rust deserializes this from JSON via serde.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedHtmlItem {
    pub tag: String,
    pub text: String,
    pub html: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub attributes: std::collections::HashMap<String, String>,

    // Optional fields (only present for specific element types)
    #[serde(default)]
    pub href: String,
    #[serde(default)]
    pub src: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub selector: String,
    #[serde(default)]
    pub table_index: u32,
    #[serde(default)]
    pub table_data: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedHtmlSummary {
    pub total_items: usize,
    pub links: Vec<ParsedHtmlItem>,
    pub images: Vec<ParsedHtmlItem>,
    pub headings: Vec<ParsedHtmlItem>,
    pub meta_tags: Vec<ParsedHtmlItem>,
    pub tables: Vec<ParsedHtmlItem>,
    pub text_blocks: Vec<ParsedHtmlItem>,
}
