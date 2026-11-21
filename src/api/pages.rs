//! `/api/pages` handlers: create, read, edit, delete, list, views.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::accounts::insert_account;
use super::dto::{
    clean_option, CreatedPageResponse, PageInput, PageListResponse, PageResponse,
    PageSummaryResponse, PageViewsResponse, MAX_CONTENT, MAX_TITLE,
};
use crate::auth::{self, MaybeUser, RequireAuth};
use crate::error::AppError;
use crate::models::{Page, PageSummary};
use crate::render::render_content;
use crate::slug;
use crate::state::AppState;

/// Fetch a page by its public path. `Ok(None)` when it does not exist.
pub async fn fetch_page(pool: &sqlx::PgPool, path: &str) -> Result<Option<Page>, AppError> {
    sqlx::query_as::<_, Page>(
        "SELECT path, account_id, title, content, content_format, author_name, \
         author_url, html_cache, views, created_at, updated_at \
         FROM pages WHERE path = $1",
    )
    .bind(path)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)
}

#[derive(Debug, Deserialize)]
pub struct GetPageParams {
    pub return_content: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

fn validate_page_input(title: &str, content: &str) -> Result<String, AppError> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::BadRequest("title must not be empty".into()));
    }
    if title.chars().count() > MAX_TITLE {
        return Err(AppError::BadRequest(format!(
            "title must be at most {MAX_TITLE} characters"
        )));
    }
    if content.len() > MAX_CONTENT {
        return Err(AppError::BadRequest(format!(
            "content must be at most {MAX_CONTENT} bytes"
        )));
    }
    Ok(title)
}

/// Create a page.
///
/// Key auth behavior: when no token is supplied, an anonymous account is
/// created on the fly (mirroring telegra.ph's zero-signup UX); its UUID is
/// returned in the `token` field and set as the `op_token` cookie so the
/// browser stays logged in.
pub async fn create_page(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Json(req): Json<PageInput>,
) -> Result<Response, AppError> {
    let title = validate_page_input(&req.title, &req.content)?;
    let author_name = clean_option(req.author_name.clone());
    let author_url = clean_option(req.author_url.clone());

    let (account_id, new_token) = match user {
        Some(acc) => (acc.id, None),
        None => {
            // Anonymous account, created implicitly on first publish.
            let short_name = author_name
                .clone()
                .filter(|n| n.chars().count() <= MAX_TITLE)
                .unwrap_or_else(|| "Anonymous".to_string());
            let id = Uuid::new_v4();
            insert_account(
                &state.pool,
                id,
                &short_name,
                author_name.clone(),
                author_url.clone(),
            )
            .await?;
            (id, Some(id))
        }
    };

    let path = slug::unique_slug(&state.pool).await?;
    let html = render_content(&req.content_format, &req.content);

    let page = sqlx::query_as::<_, Page>(
        "INSERT INTO pages (path, account_id, title, content, content_format, author_name, \
         author_url, html_cache) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING path, account_id, title, content, content_format, author_name, \
         author_url, html_cache, views, created_at, updated_at",
    )
    .bind(&path)
    .bind(account_id)
    .bind(&title)
    .bind(&req.content)
    .bind(req.content_format)
    .bind(&author_name)
    .bind(&author_url)
    .bind(&html)
    .fetch_one(&state.pool)
    .await?;

    let body = CreatedPageResponse {
        page: PageResponse::from_page(&page, &state.config.base_url, true),
        token: new_token,
    };
    let mut response = (StatusCode::CREATED, Json(body)).into_response();
    if let Some(token) = new_token {
        let cookie = auth::token_cookie(&token, state.config.base_url.starts_with("https"));
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().insert(header::SET_COOKIE, value);
        }
    }
    Ok(response)
}

/// Read a page (public; does not increment views).
pub async fn get_page(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(params): Query<GetPageParams>,
) -> Result<Json<PageResponse>, AppError> {
    let page = fetch_page(&state.pool, &path)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(PageResponse::from_page(
        &page,
        &state.config.base_url,
        params.return_content.unwrap_or(false),
    )))
}

/// Edit a page (owner only).
pub async fn edit_page(
    State(state): State<AppState>,
    RequireAuth(auth): RequireAuth,
    Path(path): Path<String>,
    Json(req): Json<PageInput>,
) -> Result<Json<PageResponse>, AppError> {
    let title = validate_page_input(&req.title, &req.content)?;
    let existing = fetch_page(&state.pool, &path)
        .await?
        .ok_or(AppError::NotFound)?;
    // 401 rather than 403 so we do not reveal whether a page exists.
    if existing.account_id != auth.id {
        return Err(AppError::Unauthorized);
    }
    let html = render_content(&req.content_format, &req.content);
    let page = sqlx::query_as::<_, Page>(
        "UPDATE pages SET title = $2, content = $3, content_format = $4, author_name = $5, \
         author_url = $6, html_cache = $7, updated_at = now() WHERE path = $1 \
         RETURNING path, account_id, title, content, content_format, author_name, \
         author_url, html_cache, views, created_at, updated_at",
    )
    .bind(&path)
    .bind(&title)
    .bind(&req.content)
    .bind(req.content_format)
    .bind(clean_option(req.author_name))
    .bind(clean_option(req.author_url))
    .bind(&html)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(PageResponse::from_page(
        &page,
        &state.config.base_url,
        true,
    )))
}

/// Delete a page (owner only).
pub async fn delete_page(
    State(state): State<AppState>,
    RequireAuth(auth): RequireAuth,
    Path(path): Path<String>,
) -> Result<StatusCode, AppError> {
    let existing = fetch_page(&state.pool, &path)
        .await?
        .ok_or(AppError::NotFound)?;
    if existing.account_id != auth.id {
        return Err(AppError::Unauthorized);
    }
    sqlx::query("DELETE FROM pages WHERE path = $1")
        .bind(&path)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// List the authenticated account's pages.
pub async fn list_pages(
    State(state): State<AppState>,
    RequireAuth(auth): RequireAuth,
    Query(params): Query<ListParams>,
) -> Result<Json<PageListResponse>, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let rows: Vec<PageSummary> = sqlx::query_as(
        "SELECT path, title, views, created_at, updated_at FROM pages \
         WHERE account_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(auth.id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM pages WHERE account_id = $1")
        .bind(auth.id)
        .fetch_one(&state.pool)
        .await?;
    let pages = rows
        .iter()
        .map(|p| PageSummaryResponse::from_summary(p, &state.config.base_url))
        .collect();
    Ok(Json(PageListResponse {
        total_count: total,
        pages,
    }))
}

/// Read a page's view counter (public).
pub async fn page_views(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<Json<PageViewsResponse>, AppError> {
    let views: Option<i64> = sqlx::query_scalar("SELECT views FROM pages WHERE path = $1")
        .bind(&path)
        .fetch_optional(&state.pool)
        .await?;
    let views = views.ok_or(AppError::NotFound)?;
    Ok(Json(PageViewsResponse { path, views }))
}
