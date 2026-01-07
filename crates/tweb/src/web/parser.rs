//! HTML parsing and page representation

use crate::web::{Form, FormField, Link};
use scraper::{Html, Selector};
use url::Url;

/// A parsed web page
#[derive(Debug, Clone)]
pub struct Page {
    /// Current URL
    pub url: String,
    /// Page title
    pub title: String,
    /// Content type
    pub content_type: String,
    /// Plain text content (rendered)
    pub content: Vec<String>,
    /// Links in the page
    pub links: Vec<Link>,
    /// Forms in the page
    pub forms: Vec<Form>,
}

impl Page {
    /// Parse HTML into a Page
    pub fn parse(url: &str, content_type: &str, body: &str) -> Self {
        if content_type.contains("text/plain") {
            return Self::plain_text(url, body);
        }

        let document = Html::parse_document(body);
        let base_url = Url::parse(url).ok();

        // Extract title
        let title = Selector::parse("title")
            .ok()
            .and_then(|sel| document.select(&sel).next())
            .map(|el| el.text().collect::<String>())
            .unwrap_or_else(|| url.to_string());

        // Extract text content and links
        let mut content = Vec::new();
        let mut links = Vec::new();
        let mut current_line = String::new();

        // Process body content
        Self::extract_content(&document, &base_url, &mut content, &mut links, &mut current_line);

        if !current_line.is_empty() {
            content.push(current_line);
        }

        // Extract forms
        let forms = Self::extract_forms(&document, &base_url);

        Self {
            url: url.to_string(),
            title,
            content_type: content_type.to_string(),
            content,
            links,
            forms,
        }
    }

    /// Create a plain text page
    fn plain_text(url: &str, body: &str) -> Self {
        Self {
            url: url.to_string(),
            title: url.to_string(),
            content_type: "text/plain".to_string(),
            content: body.lines().map(String::from).collect(),
            links: vec![],
            forms: vec![],
        }
    }

    fn extract_content(
        document: &Html,
        base_url: &Option<Url>,
        content: &mut Vec<String>,
        links: &mut Vec<Link>,
        current_line: &mut String,
    ) {
        // Simple text extraction - walk through relevant elements
        let body_selector = Selector::parse("body").unwrap();
        let skip_selector = Selector::parse("script, style, noscript, head, meta, link").unwrap();

        if let Some(body) = document.select(&body_selector).next() {
            Self::process_element(&body, base_url, content, links, current_line, &skip_selector, 0);
        }
    }

    fn process_element(
        element: &scraper::ElementRef,
        base_url: &Option<Url>,
        content: &mut Vec<String>,
        links: &mut Vec<Link>,
        current_line: &mut String,
        skip_selector: &Selector,
        depth: usize,
    ) {
        // Skip certain elements
        if element.select(skip_selector).next().is_some() && depth > 0 {
            return;
        }

        let tag_name = element.value().name();

        // Handle block elements
        let is_block = matches!(
            tag_name,
            "p" | "div" | "br" | "hr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" |
            "li" | "ul" | "ol" | "table" | "tr" | "blockquote" | "pre" | "article" |
            "section" | "header" | "footer" | "nav" | "aside"
        );

        if is_block && !current_line.is_empty() {
            content.push(std::mem::take(current_line));
        }

        // Handle headers
        if matches!(tag_name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
            content.push(String::new());
        }

        // Handle horizontal rule
        if tag_name == "hr" {
            content.push("─".repeat(40));
        }

        // Handle list items
        if tag_name == "li" {
            current_line.push_str("  • ");
        }

        // Process children
        for child in element.children() {
            match child.value() {
                scraper::Node::Text(text) => {
                    let text = text.trim();
                    if !text.is_empty() {
                        if !current_line.is_empty() && !current_line.ends_with(' ') {
                            current_line.push(' ');
                        }
                        current_line.push_str(text);
                    }
                }
                scraper::Node::Element(_) => {
                    if let Some(child_el) = scraper::ElementRef::wrap(child) {
                        let child_tag = child_el.value().name();

                        // Skip script, style, etc.
                        if matches!(child_tag, "script" | "style" | "noscript" | "head" | "meta" | "link") {
                            continue;
                        }

                        // Handle links
                        if child_tag == "a" {
                            if let Some(href) = child_el.value().attr("href") {
                                let link_text: String = child_el.text().collect();
                                let link_text = link_text.trim();

                                if !link_text.is_empty() {
                                    let full_url = resolve_url(base_url, href);
                                    let link_num = links.len() + 1;

                                    if !current_line.is_empty() && !current_line.ends_with(' ') {
                                        current_line.push(' ');
                                    }
                                    current_line.push_str(&format!("[{link_num}]{link_text}"));

                                    links.push(Link {
                                        text: link_text.to_string(),
                                        url: full_url,
                                        line: content.len(),
                                    });
                                }
                                continue;
                            }
                        }

                        // Handle images
                        if child_tag == "img" {
                            let alt = child_el.value().attr("alt").unwrap_or("[image]");
                            current_line.push_str(&format!("[{alt}]"));
                            continue;
                        }

                        Self::process_element(
                            &child_el,
                            base_url,
                            content,
                            links,
                            current_line,
                            skip_selector,
                            depth + 1,
                        );
                    }
                }
                _ => {}
            }
        }

        if is_block && !current_line.is_empty() {
            content.push(std::mem::take(current_line));
        }
    }

    fn extract_forms(document: &Html, base_url: &Option<Url>) -> Vec<Form> {
        let form_selector = Selector::parse("form").unwrap();
        let input_selector = Selector::parse("input, textarea, select").unwrap();

        document
            .select(&form_selector)
            .map(|form| {
                let action = form
                    .value()
                    .attr("action")
                    .map(|a| resolve_url(base_url, a))
                    .unwrap_or_default();

                let method = form
                    .value()
                    .attr("method")
                    .unwrap_or("GET")
                    .to_uppercase();

                let fields = form
                    .select(&input_selector)
                    .filter_map(|input| {
                        let name = input.value().attr("name")?.to_string();
                        let field_type = input.value().attr("type").unwrap_or("text").to_string();
                        let value = input.value().attr("value").unwrap_or("").to_string();
                        let placeholder = input.value().attr("placeholder").unwrap_or("").to_string();

                        Some(FormField {
                            name,
                            field_type,
                            value,
                            placeholder,
                        })
                    })
                    .collect();

                Form {
                    action,
                    method,
                    fields,
                }
            })
            .collect()
    }

    /// Get plain text content as a single string
    pub fn text_content(&self) -> String {
        self.content.join("\n")
    }
}

/// Resolve a relative URL against a base URL
fn resolve_url(base: &Option<Url>, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }

    if let Some(base) = base {
        if let Ok(resolved) = base.join(href) {
            return resolved.to_string();
        }
    }

    href.to_string()
}
