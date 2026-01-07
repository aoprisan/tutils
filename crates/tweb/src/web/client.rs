//! HTTP client for web browsing

use crate::config::TwebConfig;
use crate::web::Page;
use anyhow::{Context, Result};
use reqwest::redirect::Policy;
use std::time::Duration;

/// HTTP client for fetching web pages
pub struct WebClient {
    client: reqwest::Client,
}

impl WebClient {
    /// Create a new web client
    pub fn new(config: &TwebConfig) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(&config.user_agent)
            .timeout(Duration::from_secs(config.timeout))
            .redirect(Policy::limited(config.max_redirects))
            .gzip(true)
            .brotli(true)
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// Fetch a page from URL
    pub async fn fetch(&self, url: &str) -> Result<Page> {
        let url = normalize_url(url)?;

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("Failed to fetch {url}"))?;

        let final_url = response.url().to_string();
        let status = response.status();

        if !status.is_success() {
            anyhow::bail!("HTTP {}: {}", status.as_u16(), status.canonical_reason().unwrap_or("Unknown"));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();

        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        Ok(Page::parse(&final_url, &content_type, &body))
    }

    /// Fetch with POST data
    pub async fn post(&self, url: &str, data: &[(String, String)]) -> Result<Page> {
        let url = normalize_url(url)?;

        let response = self
            .client
            .post(&url)
            .form(data)
            .send()
            .await
            .with_context(|| format!("Failed to post to {url}"))?;

        let final_url = response.url().to_string();
        let status = response.status();

        if !status.is_success() {
            anyhow::bail!("HTTP {}: {}", status.as_u16(), status.canonical_reason().unwrap_or("Unknown"));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();

        let body = response.text().await.context("Failed to read response body")?;

        Ok(Page::parse(&final_url, &content_type, &body))
    }
}

/// Normalize a URL (add https:// if missing)
fn normalize_url(url: &str) -> Result<String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        Ok(url.to_string())
    } else if url.starts_with("//") {
        Ok(format!("https:{url}"))
    } else if url.contains('.') && !url.contains(' ') {
        // Looks like a domain
        Ok(format!("https://{url}"))
    } else {
        anyhow::bail!("Invalid URL: {url}")
    }
}
