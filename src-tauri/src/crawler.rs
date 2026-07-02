use crate::models::*;
use crate::request_clients;
use scraper::{Html, Selector};

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
