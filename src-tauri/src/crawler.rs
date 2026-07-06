use crate::models::*;
use crate::request_clients;
use scraper::{Html, Selector};

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_html() -> &'static str {
        r#"<html><body>
            <h1>Test Title</h1>
            <p class="desc">Hello World</p>
            <div id="content">
                <a href="https://example.com" class="link">Click here</a>
                <img src="pic.jpg" alt="A photo" class="photo" />
                <ul class="items">
                    <li class="item" data-id="1">Item One</li>
                    <li class="item" data-id="2">Item Two</li>
                    <li class="item" data-id="3">Item Three</li>
                </ul>
            </div>
            <footer>Footer text</footer>
        </body></html>"#
    }

    #[test]
    fn test_strip_html_tags_removes_tags() {
        let html = sample_html();
        let text = strip_html_tags(html);
        assert!(text.contains("Test Title"));
        assert!(text.contains("Hello World"));
        assert!(text.contains("Item One"));
        assert!(!text.contains("<h1>"));
        assert!(!text.contains("<p"));
        assert!(!text.contains("</a>"));
    }

    #[test]
    fn test_strip_html_tags_empty() {
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn test_strip_html_tags_no_html() {
        assert_eq!(strip_html_tags("plain text"), "plain text");
    }

    #[test]
    fn test_strip_html_tags_script_style() {
        let html = r#"<html><body><p>Hello</p><script>alert('x')</script><style>.c{}</style></body></html>"#;
        let text = strip_html_tags(html);
        assert!(text.contains("Hello"), "text should contain paragraph text: {:?}", text);
    }

    #[test]
    fn test_extract_from_html_valid_selector() {
        let rules = vec![
            ExtractRule {
                field: "title".into(),
                selector: "h1".into(),
                attribute: None,
                extract_multiple: None,
            },
        ];
        let results = extract_from_html(sample_html(), &rules);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].field, "title");
        assert_eq!(results[0].values, vec!["Test Title"]);
    }

    #[test]
    fn test_extract_from_html_class_selector() {
        let rules = vec![
            ExtractRule {
                field: "desc".into(),
                selector: ".desc".into(),
                attribute: None,
                extract_multiple: None,
            },
        ];
        let results = extract_from_html(sample_html(), &rules);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].values, vec!["Hello World"]);
    }

    #[test]
    fn test_extract_from_html_attribute() {
        let rules = vec![
            ExtractRule {
                field: "link".into(),
                selector: "a.link".into(),
                attribute: Some("href".into()),
                extract_multiple: None,
            },
        ];
        let results = extract_from_html(sample_html(), &rules);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].values, vec!["https://example.com"]);
    }

    #[test]
    fn test_extract_from_html_img_src() {
        let rules = vec![
            ExtractRule {
                field: "image".into(),
                selector: "img.photo".into(),
                attribute: Some("src".into()),
                extract_multiple: None,
            },
        ];
        let results = extract_from_html(sample_html(), &rules);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].values, vec!["pic.jpg"]);
    }

    #[test]
    fn test_extract_from_html_multiple_items() {
        let rules = vec![
            ExtractRule {
                field: "items".into(),
                selector: "li.item".into(),
                attribute: None,
                extract_multiple: Some(true),
            },
        ];
        let results = extract_from_html(sample_html(), &rules);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].values, vec!["Item One", "Item Two", "Item Three"]);
    }

    #[test]
    fn test_extract_from_html_single_item_with_multiple_false() {
        let rules = vec![
            ExtractRule {
                field: "items".into(),
                selector: "li.item".into(),
                attribute: None,
                extract_multiple: Some(false),
            },
        ];
        let results = extract_from_html(sample_html(), &rules);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].values, vec!["Item One"]);
    }

    #[test]
    fn test_extract_from_html_invalid_selector() {
        let rules = vec![
            ExtractRule {
                field: "bad".into(),
                selector: "!!!invalid!!!".into(),
                attribute: None,
                extract_multiple: None,
            },
        ];
        let results = extract_from_html(sample_html(), &rules);
        assert_eq!(results.len(), 1);
        assert!(results[0].values[0].starts_with("Invalid selector:"));
    }

    #[test]
    fn test_extract_from_html_selector_not_found() {
        let rules = vec![
            ExtractRule {
                field: "missing".into(),
                selector: ".nonexistent".into(),
                attribute: None,
                extract_multiple: None,
            },
        ];
        let results = extract_from_html(sample_html(), &rules);
        assert_eq!(results.len(), 1);
        assert!(results[0].values.is_empty());
    }

    #[test]
    fn test_extract_from_html_nested_selector() {
        let rules = vec![
            ExtractRule {
                field: "nested".into(),
                selector: "#content a".into(),
                attribute: None,
                extract_multiple: None,
            },
        ];
        let results = extract_from_html(sample_html(), &rules);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].values, vec!["Click here"]);
    }

    #[test]
    fn test_extract_from_html_alt_attribute() {
        let rules = vec![
            ExtractRule {
                field: "alt_text".into(),
                selector: "img.photo".into(),
                attribute: Some("alt".into()),
                extract_multiple: None,
            },
        ];
        let results = extract_from_html(sample_html(), &rules);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].values, vec!["A photo"]);
    }

    #[test]
    fn test_extract_from_html_empty_html() {
        let rules = vec![
            ExtractRule {
                field: "test".into(),
                selector: "h1".into(),
                attribute: None,
                extract_multiple: None,
            },
        ];
        let results = extract_from_html("", &rules);
        assert_eq!(results.len(), 1);
        assert!(results[0].values.is_empty());
    }
}

pub async fn fetch_url(request: CrawlRequest) -> CrawlResult {
    let profile = request.client_profile.unwrap_or_default();

    if profile.client_type == "chrome" || request.use_browser.unwrap_or(false) {
        let chrome_profile = ClientProfile {
            client_type: "chrome".into(),
            user_agent: profile.user_agent,
            proxy_url: profile.proxy_url,
            headers: profile.headers,
            timeout_secs: profile.timeout_secs,
            profile_dir: profile.profile_dir,
            chrome_args: profile.chrome_args,
            wait_for_selector: request.wait_for_selector.clone().or(profile.wait_for_selector),
            extra_nav_args: profile.extra_nav_args,
            headless: profile.headless,
        };
        return request_clients::fetch_with_client(&request.url, &chrome_profile, request.extract_rules).await;
    }

    request_clients::fetch_with_client(&request.url, &profile, request.extract_rules).await
}

pub fn extract_from_html(html: &str, rules: &[ExtractRule]) -> Vec<ExtractedField> {
    let document = Html::parse_document(html);
    let mut results = Vec::new();

    for rule in rules {
        let selector_str = &rule.selector;
        if let Ok(selector) = Selector::parse(selector_str) {
            let mut values = Vec::new();
            for element in document.select(&selector) {
                let value = if let Some(attr) = &rule.attribute {
                    element.value().attr(attr).unwrap_or("").to_string()
                } else {
                    element.text().collect::<Vec<_>>().join(" ").trim().to_string()
                };
                values.push(value);
                if !rule.extract_multiple.unwrap_or(false) {
                    break;
                }
            }
            results.push(ExtractedField {
                field: rule.field.clone(),
                values,
            });
        } else {
            results.push(ExtractedField {
                field: rule.field.clone(),
                values: vec![format!("Invalid selector: {}", selector_str)],
            });
        }
    }

    results
}

pub fn strip_html_tags(html: &str) -> String {
    let document = Html::parse_document(html);
    document.root_element().text().collect::<Vec<_>>().join(" ")
}

pub async fn batch_crawl(urls: Vec<String>, rules: Vec<ExtractRule>) -> Vec<CrawlResult> {
    let mut results = Vec::new();
    for url in urls {
        let result = fetch_url(CrawlRequest {
            url,
            method: None,
            headers: None,
            body: None,
            use_browser: None,
            wait_for_selector: None,
            extract_rules: Some(rules.clone()),
            client_profile: None,
        })
        .await;
        results.push(result);
    }
    results
}
