use regex::Regex;
use serde::{Deserialize, Serialize};

// ── Pattern Types ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchPattern {
    Wildcard(String),
    Regex(String),
    Contains(String),
    StartsWith(String),
    EndsWith(String),
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchRule {
    pub field: String,
    pub pattern: MatchPattern,
    #[serde(default)]
    pub negate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub matched: bool,
    pub rule_index: usize,
}

// ── Matcher Engine ─────────────────────────────────────────

pub struct ItemMatcher;

impl ItemMatcher {
    /// Kiem tra 1 item co match voi rules hay khong
    pub fn matches(rules: &[MatchRule], item: &serde_json::Value) -> MatchResult {
        for (i, rule) in rules.iter().enumerate() {
            let field_value = match rule.field.as_str() {
                "url" | "source_url" => item.get("source_url")
                    .or_else(|| item.get("url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                "extracted_url" => item.get("extracted_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                "raw_content" => item.get("raw_content")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                "item_type" => item.get("item_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
                other => item.get(other)
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            };

            let matched = Self::match_pattern(&rule.pattern, field_value);
            let final_match = if rule.negate { !matched } else { matched };

            if !final_match {
                return MatchResult { matched: false, rule_index: i };
            }
        }

        MatchResult { matched: true, rule_index: rules.len().saturating_sub(1) }
    }

    fn match_pattern(pattern: &MatchPattern, value: &str) -> bool {
        match pattern {
            MatchPattern::Always => true,
            MatchPattern::Contains(s) => value.contains(s),
            MatchPattern::StartsWith(s) => value.starts_with(s),
            MatchPattern::EndsWith(s) => value.ends_with(s),
            MatchPattern::Wildcard(pat) => Self::match_wildcard(pat, value),
            MatchPattern::Regex(pat) => {
                Regex::new(pat).map(|re| re.is_match(value)).unwrap_or(false)
            }
        }
    }

    /// Wildcard matching: * = any chars, ? = single char
    fn match_wildcard(pattern: &str, value: &str) -> bool {
        let regex_pattern = format!(
            "^{}$",
            regex::escape(pattern)
                .replace(r"\*", ".*")
                .replace(r"\?", ".")
        );
        Regex::new(&regex_pattern)
            .map(|re| re.is_match(value))
            .unwrap_or(false)
    }

    /// Kiem tra 1 URL/item co match voi data extraction patterns khong
    /// (dùng trong Phase 1: URL extraction từ HTML)
    pub fn extract_matching_urls(
        html: &str,
        base_url: &str,
        patterns: &[MatchPattern],
    ) -> Vec<String> {
        let mut urls = Vec::new();
        // Extract all URLs from HTML
        for m in regex::Regex::new(r#"<a[^>]*href\s*=\s*["']([^"']+)["']"#)
            .unwrap()
            .find_iter(html)
        {
            let href = m.as_str();
            let url = Self::extract_href(href);
            if let Some(full_url) = Self::resolve_url(&url, base_url) {
                if patterns.is_empty() || patterns.iter().any(|p| Self::match_pattern(p, &full_url)) {
                    urls.push(full_url);
                }
            }
        }
        urls
    }

    fn extract_href(anchor_tag: &str) -> String {
        if let Some(cap) = regex::Regex::new(r#"href\s*=\s*["']([^"']+)["']"#)
            .unwrap()
            .captures(anchor_tag)
        {
            cap[1].to_string()
        } else {
            String::new()
        }
    }

    fn resolve_url(url: &str, base: &str) -> Option<String> {
        if url.is_empty() || url.starts_with('#') || url.starts_with("javascript:") {
            return None;
        }
        if url.starts_with("http://") || url.starts_with("https://") {
            return Some(url.to_string());
        }
        let base = base.trim_end_matches('/');
        if url.starts_with('/') {
            let domain = regex::Regex::new(r"^(https?://[^/]+)")
                .unwrap()
                .captures(base)
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| base.to_string());
            Some(format!("{}{}", domain, url))
        } else {
            Some(format!("{}/{}", base, url))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_match() {
        assert!(ItemMatcher::match_pattern(
            &MatchPattern::Wildcard("https://example.com/*".into()),
            "https://example.com/products/123"
        ));
        assert!(!ItemMatcher::match_pattern(
            &MatchPattern::Wildcard("https://other.com/*".into()),
            "https://example.com/test"
        ));
    }

    #[test]
    fn test_regex_match() {
        assert!(ItemMatcher::match_pattern(
            &MatchPattern::Regex(r"^https://.*\.example\.com/.*".into()),
            "https://shop.example.com/products/1"
        ));
    }

    #[test]
    fn test_contains_match() {
        assert!(ItemMatcher::match_pattern(
            &MatchPattern::Contains("/product/".into()),
            "https://example.com/product/123"
        ));
    }

    #[test]
    fn test_starts_with() {
        assert!(ItemMatcher::match_pattern(
            &MatchPattern::StartsWith("https://blog".into()),
            "https://blog.example.com/post"
        ));
    }

    #[test]
    fn test_ends_with() {
        assert!(ItemMatcher::match_pattern(
            &MatchPattern::EndsWith(".pdf".into()),
            "document.pdf"
        ));
    }

    #[test]
    fn test_rule_matching() {
        let rules = vec![
            MatchRule {
                field: "source_url".into(),
                pattern: MatchPattern::Contains("/product/".into()),
                negate: false,
            },
        ];
        let item = serde_json::json!({
            "source_url": "https://example.com/product/123"
        });
        let result = ItemMatcher::matches(&rules, &item);
        assert!(result.matched);
    }

    #[test]
    fn test_negated_rule() {
        let rules = vec![
            MatchRule {
                field: "source_url".into(),
                pattern: MatchPattern::Contains("/admin/".into()),
                negate: true,
            },
        ];
        let item = serde_json::json!({
            "source_url": "https://example.com/products"
        });
        let result = ItemMatcher::matches(&rules, &item);
        assert!(result.matched);
    }

    #[test]
    fn test_url_extraction() {
        let html = r#"<a href="/product/1">Product 1</a>
                      <a href="https://other.com">Other</a>
                      <a href="/blog/post">Blog</a>"#;
        let patterns = vec![
            MatchPattern::Contains("/product/".into()),
        ];
        let urls = ItemMatcher::extract_matching_urls(html, "https://example.com", &patterns);
        assert_eq!(urls.len(), 1);
        assert!(urls[0].contains("/product/1"));
    }

    #[test]
    fn test_oreka_url_extraction_with_regex() {
        let html = r#"<a href="/mua-ban-dong-ho/moi--detail/1112311">Đồng hồ mới</a>
                      <a href="https://www.oreka.vn/mua-ban-sach/sach-hay--detail/2223422">Sách hay</a>
                      <a href="/mua-ban-dien-tu/laptop--detail/3334533">Laptop</a>
                      <a href="/store/C21AVGZS44L3UU">Cửa hàng</a>
                      <a href="/blog/post">Blog</a>"#;
        let patterns = vec![
            MatchPattern::Regex(".*-detail\\/[0-9]{1,}\\/?".into()),
        ];
        let urls = ItemMatcher::extract_matching_urls(html, "https://www.oreka.vn", &patterns);
        assert_eq!(urls.len(), 3);
        assert!(urls[0].contains("--detail/1112311"));
        assert!(urls[1].contains("--detail/2223422"));
        assert!(urls[2].contains("--detail/3334533"));
    }

    #[test]
    fn test_oreka_python_plugin_regex() {
        let pattern = r##"/mua-ban(?:-[^/"']+)?/[^"']*?--detail/\d+"##;
        let re = regex::Regex::new(pattern).unwrap();
        assert!(re.is_match("/mua-ban-dong-ho/moi--detail/1112311"));
        assert!(re.is_match("https://www.oreka.vn/mua-ban-sach/sach-hay--detail/2223422"));
        assert!(re.is_match("/mua-ban-dien-tu/laptop--detail/3334533"));
        assert!(!re.is_match("/store/C21AVGZS44L3UU"));
        assert!(!re.is_match("/blog/post"));
    }

    #[test]
    fn test_extract_matching_urls_empty_html() {
        let html = "<html><body><p>No links here</p></body></html>";
        let patterns = vec![MatchPattern::Regex(".*-detail\\/[0-9]{1,}\\/?".into())];
        let urls = ItemMatcher::extract_matching_urls(html, "https://example.com", &patterns);
        assert!(urls.is_empty());
    }
}
