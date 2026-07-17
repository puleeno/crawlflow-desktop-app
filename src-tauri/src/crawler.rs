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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
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
                ..Default::default()
            },
        ];
        let results = extract_from_html("", &rules);
        assert_eq!(results.len(), 1);
        assert!(results[0].values.is_empty());
    }

    #[test]
    fn test_extract_from_html_json_ld_single() {
        let html = r#"<html><body>
            <script type="application/ld+json">
            {
                "@context": "https://schema.org",
                "@type": "Product",
                "name": "Single Product",
                "offers": {
                    "price": "99.99",
                    "sku": "SKU-SINGLE"
                }
            }
            </script>
        </body></html>"#;
        let rules = vec![
            ExtractRule {
                field: "price".into(),
                selector: "offers.price".into(),
                attribute: None,
                extract_multiple: None,
                extract_from: Some("json-ld".into()),
                json_path: Some("offers.price".into()),
            },
            ExtractRule {
                field: "sku".into(),
                selector: "offers.sku".into(),
                attribute: None,
                extract_multiple: None,
                extract_from: Some("json-ld".into()),
                json_path: Some("offers.sku".into()),
            },
        ];
        let results = extract_from_html(html, &rules);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].field, "price");
        assert_eq!(results[0].values, vec!["99.99"]);
        assert_eq!(results[1].field, "sku");
        assert_eq!(results[1].values, vec!["SKU-SINGLE"]);
    }

    #[test]
    fn test_extract_from_html_json_ld_multiple() {
        let html = r#"<html><body>
            <script type="application/ld+json">
            {
                "@context": "https://schema.org",
                "@type": "BreadcrumbList",
                "itemListElement": []
            }
            </script>
            <script type="application/ld+json">
            {
                "@context": "https://schema.org",
                "@type": "Product",
                "name": "Multiple Product",
                "offers": [
                    { "price": "10.00", "sku": "SKU-1" },
                    { "price": "20.00", "sku": "SKU-2" }
                ]
            }
            </script>
        </body></html>"#;
        let rules = vec![
            ExtractRule {
                field: "price".into(),
                selector: "offers.price".into(),
                attribute: None,
                extract_multiple: Some(true),
                extract_from: Some("json-ld".into()),
                json_path: Some("offers.price".into()),
            },
            ExtractRule {
                field: "sku_first".into(),
                selector: "offers.0.sku".into(),
                attribute: None,
                extract_multiple: None,
                extract_from: Some("json-ld".into()),
                json_path: Some("offers.0.sku".into()),
            },
        ];
        let results = extract_from_html(html, &rules);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].field, "price");
        assert_eq!(results[0].values, vec!["10.00", "20.00"]);
        assert_eq!(results[1].field, "sku_first");
        assert_eq!(results[1].values, vec!["SKU-1"]);
    }

    #[test]
    fn test_real_oreka_html() {
        use std::fs;
        let html_path = "../product_detail_sample.html";
        if let Ok(html) = fs::read_to_string(html_path) {
            let rules = vec![
                ExtractRule {
                    field: "product_name".into(),
                    selector: "h1".into(),
                    attribute: None,
                    extract_multiple: None,
                    extract_from: Some("html-element".into()),
                    json_path: None,
                },
                ExtractRule {
                    field: "price".into(),
                    selector: "offers.price".into(),
                    attribute: None,
                    extract_multiple: None,
                    extract_from: Some("json-ld".into()),
                    json_path: Some("offers.price".into()),
                },
                ExtractRule {
                    field: "sku".into(),
                    selector: "offers.sku".into(),
                    attribute: None,
                    extract_multiple: None,
                    extract_from: Some("json-ld".into()),
                    json_path: Some("offers.sku".into()),
                },
                ExtractRule {
                    field: "image_url".into(),
                    selector: "image".into(),
                    attribute: None,
                    extract_multiple: None,
                    extract_from: Some("json-ld".into()),
                    json_path: Some("image".into()),
                },
            ];
            let results = extract_from_html(&html, &rules);
            println!("Extraction results from real HTML: {:?}", results);
            
            let name_res = results.iter().find(|r| r.field == "product_name").unwrap();
            assert!(name_res.values[0].contains("FROM CRISIS TO CALLING"));

            let price_res = results.iter().find(|r| r.field == "price").unwrap();
            assert_eq!(price_res.values, vec!["250000"]);

            let sku_res = results.iter().find(|r| r.field == "sku").unwrap();
            assert!(!sku_res.values.is_empty(), "SKU should not be empty!");
            println!("Extracted SKU: {:?}", sku_res.values);

            let img_res = results.iter().find(|r| r.field == "image_url").unwrap();
            assert!(!img_res.values.is_empty(), "Image URL should not be empty!");
            println!("Extracted Image URL: {:?}", img_res.values);
        } else {
            println!("Skipping real HTML test as product_detail_sample.html was not found");
        }
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
        return request_clients::fetch_with_client(&request.url, &chrome_profile, request.extract_rules, None, None, None).await;
    }

    request_clients::fetch_with_client(&request.url, &profile, request.extract_rules, None, None, None).await
}

fn get_json_ld_objects(root: serde_json::Value) -> Vec<serde_json::Value> {
    let mut objects = Vec::new();
    match root {
        serde_json::Value::Array(arr) => {
            for item in arr {
                objects.extend(get_json_ld_objects(item));
            }
        }
        serde_json::Value::Object(mut obj) => {
            if let Some(serde_json::Value::Array(graph)) = obj.remove("@graph") {
                for item in graph {
                    objects.extend(get_json_ld_objects(item));
                }
            } else {
                objects.push(serde_json::Value::Object(obj));
            }
        }
        _ => {}
    }
    objects
}

fn extract_json_path(value: &serde_json::Value, path_parts: &[&str]) -> Vec<String> {
    if path_parts.is_empty() {
        match value {
            serde_json::Value::Null => return vec![],
            serde_json::Value::Bool(b) => return vec![b.to_string()],
            serde_json::Value::Number(n) => return vec![n.to_string()],
            serde_json::Value::String(s) => return vec![s.clone()],
            serde_json::Value::Array(arr) => {
                let mut res = Vec::new();
                for item in arr {
                    res.extend(extract_json_path(item, path_parts));
                }
                return res;
            }
            serde_json::Value::Object(obj) => {
                if let Some(val) = obj.get("url").or_else(|| obj.get("@value")) {
                    return extract_json_path(val, path_parts);
                }
                return vec![value.to_string()];
            }
        }
    }

    let first = path_parts[0];
    let rest = &path_parts[1..];

    match value {
        serde_json::Value::Object(obj) => {
            if let Some(sub_val) = obj.get(first) {
                extract_json_path(sub_val, rest)
            } else {
                vec![]
            }
        }
        serde_json::Value::Array(arr) => {
            if let Ok(idx) = first.parse::<usize>() {
                if let Some(item) = arr.get(idx) {
                    extract_json_path(item, rest)
                } else {
                    vec![]
                }
            } else {
                let mut res = Vec::new();
                for item in arr {
                    res.extend(extract_json_path(item, path_parts));
                }
                res
            }
        }
        _ => vec![]
    }
}

pub fn extract_from_html(html: &str, rules: &[ExtractRule]) -> Vec<ExtractedField> {
    let document = Html::parse_document(html);
    let mut results = Vec::new();

    let mut json_ld_parsed: Option<Vec<serde_json::Value>> = None;

    for rule in rules {
        if rule.extract_from.as_deref() == Some("json-ld") {
            let json_objects = json_ld_parsed.get_or_insert_with(|| {
                let mut objects = Vec::new();
                if let Ok(script_selector) = Selector::parse("script[type=\"application/ld+json\"]") {
                    for script_element in document.select(&script_selector) {
                        let script_text = script_element.text().collect::<Vec<_>>().join("");
                        if let Ok(parsed_json) = serde_json::from_str::<serde_json::Value>(&script_text) {
                            objects.extend(get_json_ld_objects(parsed_json));
                        }
                    }
                }
                objects
            });

            let target_path = rule.json_path.as_deref().unwrap_or(&rule.selector);
            let path_parts: Vec<&str> = target_path.split('.').filter(|s| !s.is_empty()).collect();
            let mut values = Vec::new();

            for obj in json_objects {
                let found = extract_json_path(obj, &path_parts);
                if !found.is_empty() {
                    values.extend(found);
                    if !rule.extract_multiple.unwrap_or(false) {
                        break;
                    }
                }
            }

            results.push(ExtractedField {
                field: rule.field.clone(),
                values,
            });
        } else {
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
