use crate::models::{PaginationConfig, StopCondition};
use crate::request_clients;
use crate::models::ClientProfile;
use std::collections::HashSet;
use scraper::Selector;

/// Context for pagination - contains state from current page
#[derive(Debug, Clone)]
pub struct PageContext {
    pub url: String,
    pub html: String,
    pub page_num: u32,
    pub seen_hashes: HashSet<String>,
    pub consecutive_no_new: u32,
}

/// Action to take for next page
#[derive(Debug, Clone)]
pub enum PageAction {
    FetchUrl(String),
    Stop,
}

/// Trait for pagination strategies
pub trait PaginationStrategy: Send + Sync {
    fn next_page(&self, context: &PageContext) -> Option<PageAction>;
    fn has_more(&self, context: &PageContext) -> bool;
}

/// URL parameter pagination: ?page=1, ?page=2, etc.
pub struct UrlParameterPagination {
    config: PaginationConfig,
}

impl UrlParameterPagination {
    pub fn new(config: PaginationConfig) -> Self {
        Self { config }
    }
}

impl PaginationStrategy for UrlParameterPagination {
    fn next_page(&self, context: &PageContext) -> Option<PageAction> {
        let param = self.config.param.as_deref().unwrap_or("page");
        let _start = self.config.start.unwrap_or(1);
        let step = self.config.step.unwrap_or(1);
        
        let next_page_num = context.page_num + step;
        
        // Build next URL
        let separator = if context.url.contains('?') { '&' } else { '?' };
        let next_url = format!("{}{}{}={}", context.url, separator, param, next_page_num);
        
        Some(PageAction::FetchUrl(next_url))
    }

    fn has_more(&self, context: &PageContext) -> bool {
        match &self.config.stop_condition {
            StopCondition::MaxPages { max_pages } => {
                context.page_num < *max_pages
            }
            StopCondition::NoNewData => {
                // Check if current page hash is already seen
                let current_hash = hash_html(&context.html);
                !context.seen_hashes.contains(&current_hash)
            }
            StopCondition::SelectorMissing { selector } => {
                // Check if selector exists in current page
                selector_exists(&context.html, selector)
            }
            StopCondition::NoDuplicatesAfter { count } => {
                context.consecutive_no_new < *count
            }
        }
    }
}

/// Helper: hash HTML content for duplicate detection
fn hash_html(html: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    html.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// Helper: check if CSS selector exists in HTML
fn selector_exists(html: &str, selector: &str) -> bool {
    use scraper::Html;
    let document = Html::parse_document(html);
    let sel = Selector::parse(selector).unwrap_or_else(|_| Selector::parse("body").unwrap());
    document.select(&sel).next().is_some()
}

/// Execute pagination with given strategy
pub async fn execute_pagination(
    base_url: &str,
    config: &PaginationConfig,
    profile: &ClientProfile,
    strategy: &dyn PaginationStrategy,
) -> Result<Vec<String>, String> {
    let mut all_htmls = Vec::new();
    let mut context = PageContext {
        url: base_url.to_string(),
        html: String::new(),
        page_num: config.start.unwrap_or(1) - 1,
        seen_hashes: HashSet::new(),
        consecutive_no_new: 0,
    };

    loop {
        // Fetch current page
        let result = request_clients::fetch_with_client(
            &context.url,
            profile,
            None,
            None,
            None,
            None,
        ).await;

        let html = match result.html {
            Some(h) => h,
            None => {
                let err = result.error.unwrap_or_else(|| "Unknown error".to_string());
                return Err(format!("Failed to fetch page {}: {}", context.page_num + 1, err));
            }
        };

        // Check for duplicates
        let current_hash = hash_html(&html);
        let is_duplicate = context.seen_hashes.contains(&current_hash);
        
        if is_duplicate {
            context.consecutive_no_new += 1;
        } else {
            context.consecutive_no_new = 0;
            context.seen_hashes.insert(current_hash.clone());
            all_htmls.push(html.clone());
        }

        context.html = html;
        context.page_num += 1;

        // Check if should continue
        if !strategy.has_more(&context) {
            break;
        }

        // Get next page action
        match strategy.next_page(&context) {
            Some(PageAction::FetchUrl(next_url)) => {
                context.url = next_url;
            }
            Some(PageAction::Stop) => break,
            None => break,
        }
    }

    Ok(all_htmls)
}
