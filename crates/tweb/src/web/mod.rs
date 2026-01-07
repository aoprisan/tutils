//! Web browsing functionality

mod client;
mod parser;
mod render;

pub use client::WebClient;
pub use parser::Page;
pub use render::PageRenderer;

/// A link in a page
#[derive(Debug, Clone)]
pub struct Link {
    /// Link text
    pub text: String,
    /// Target URL
    pub url: String,
    /// Position in rendered content (line number)
    pub line: usize,
}

/// A form in a page
#[derive(Debug, Clone)]
pub struct Form {
    /// Form action URL
    pub action: String,
    /// HTTP method (GET/POST)
    pub method: String,
    /// Form fields
    pub fields: Vec<FormField>,
}

/// A form field
#[derive(Debug, Clone)]
pub struct FormField {
    /// Field name
    pub name: String,
    /// Field type (text, password, hidden, submit, etc.)
    pub field_type: String,
    /// Current value
    pub value: String,
    /// Placeholder text
    pub placeholder: String,
}

/// Navigation history entry
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub scroll_position: usize,
}
