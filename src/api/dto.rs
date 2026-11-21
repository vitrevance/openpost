//! Wire (request/response) types for the REST API.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{ContentFormat, Page, PageSummary};

pub const MAX_TITLE: usize = 256;
pub const MAX_CONTENT: usize = 256 * 1024;
pub const MAX_SHORT_NAME: usize = 64;

/// Normalize an optional string: trim it, and map empty strings to `None`.
pub fn clean_option(s: Option<String>) -> Option<String> {
    let t = s?.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

// --- accounts ---

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    pub short_name: String,
    pub author_name: Option<String>,
    pub author_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAccountRequest {
    pub short_name: Option<String>,
    pub author_name: Option<String>,
    pub author_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    /// The account id — which is also the access token. Clients MUST
    /// persist it; there is no recovery mechanism.
    pub id: Uuid,
    pub short_name: String,
    pub author_name: Option<String>,
    pub author_url: Option<String>,
    /// Unix timestamp (seconds).
    pub created_at: i64,
}

impl From<crate::models::Account> for AccountResponse {
    fn from(a: crate::models::Account) -> Self {
        Self {
            id: a.id,
            short_name: a.short_name,
            author_name: a.author_name,
            author_url: a.author_url,
            created_at: a.created_at.unix_timestamp(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub token: String,
}

// --- pages ---

/// Payload for creating and editing a page.
#[derive(Debug, Deserialize)]
pub struct PageInput {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub content_format: ContentFormat,
    pub author_name: Option<String>,
    pub author_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PageResponse {
    pub path: String,
    pub url: String,
    pub title: String,
    pub author_name: Option<String>,
    pub author_url: Option<String>,
    pub content_format: ContentFormat,
    pub views: i64,
    /// Unix timestamp (seconds).
    pub created_at: i64,
    /// Unix timestamp (seconds).
    pub updated_at: i64,
    /// Raw source content; only included when `return_content` is requested.
    pub content: Option<String>,
}

impl PageResponse {
    pub fn from_page(p: &Page, base_url: &str, include_content: bool) -> Self {
        Self {
            path: p.path.clone(),
            url: format!("{base_url}/{}", p.path),
            title: p.title.clone(),
            author_name: p.author_name.clone(),
            author_url: p.author_url.clone(),
            content_format: p.content_format,
            views: p.views,
            created_at: p.created_at.unix_timestamp(),
            updated_at: p.updated_at.unix_timestamp(),
            content: if include_content {
                Some(p.content.clone())
            } else {
                None
            },
        }
    }
}

/// Response for `POST /api/pages`: the new page, plus — when an account was
/// created implicitly — the new access token the client must save.
#[derive(Debug, Serialize)]
pub struct CreatedPageResponse {
    #[serde(flatten)]
    pub page: PageResponse,
    pub token: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct PageSummaryResponse {
    pub path: String,
    pub url: String,
    pub title: String,
    pub views: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl PageSummaryResponse {
    pub fn from_summary(p: &PageSummary, base_url: &str) -> Self {
        Self {
            path: p.path.clone(),
            url: format!("{base_url}/{}", p.path),
            title: p.title.clone(),
            views: p.views,
            created_at: p.created_at.unix_timestamp(),
            updated_at: p.updated_at.unix_timestamp(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PageListResponse {
    pub total_count: i64,
    pub pages: Vec<PageSummaryResponse>,
}

#[derive(Debug, Serialize)]
pub struct PageViewsResponse {
    pub path: String,
    pub views: i64,
}

// --- preview ---

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub content_format: ContentFormat,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    pub html: String,
}
