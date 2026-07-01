use crate::models::*;
use scraper::{Html, Selector};

pub async fn fetch_url(request: CrawlRequest) -> CrawlResult {
    let client = reqwest::Client::builder()
        .user_agent("CrawlFlow/1.0")
        .danger_accept_invalid_certs(false)
        .build()
        .map_err(|e| e.to_string());

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return CrawlResult {
                url: request.url.clone(),
                status: 0,
                html: None,
                text: None,
                extracted: None,
                error: Some(format!("Failed to create HTTP client: {}", e)),
            }
        }
    };

    let mut req = client.get(&request.url);
    if let Some(headers) = &request.headers {
        for h in headers {
            req = req.header(&h.key, &h.value);
        }
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return CrawlResult {
                url: request.url.clone(),
                status: 0,
                html: None,
                text: None,
                extracted: None,
                error: Some(format!("HTTP request failed: {}", e)),
            }
        }
    };

    let status = response.status().as_u16();
    let html = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            return CrawlResult {
                url: request.url.clone(),
                status,
                html: None,
                text: None,
                extracted: None,
                error: Some(format!("Failed to read response body: {}", e)),
            }
        }
    };

    let text = strip_html_tags(&html);

    let extracted = if let Some(rules) = &request.extract_rules {
        Some(extract_from_html(&html, rules))
    } else {
        None
    };

    CrawlResult {
        url: request.url,
        status,
        html: Some(html),
        text: Some(text),
        extracted,
        error: None,
    }
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
        })
        .await;
        results.push(result);
    }
    results
}
