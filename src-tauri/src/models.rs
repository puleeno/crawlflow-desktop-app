use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlRequest {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<Vec<HeaderPair>>,
    pub body: Option<String>,
    pub use_browser: Option<bool>,
    pub wait_for_selector: Option<String>,
    pub extract_rules: Option<Vec<ExtractRule>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeaderPair {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractRule {
    pub field: String,
    pub selector: String,
    pub attribute: Option<String>,
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
