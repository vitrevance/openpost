//! HTML view handlers for the web UI.

use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Serialize;
use uuid::Uuid;

use crate::api::pages::fetch_page;
use crate::auth::{self, clear_token_cookie, MaybeUser, RequireAuth};
use crate::models::PageSummary;
use crate::state::AppState;

fn render(state: &AppState, template: &str, ctx: &tera::Context) -> Result<Html<String>, String> {
    state
        .tera
        .render(template, ctx)
        .map(Html)
        .map_err(|e| e.to_string())
}

fn base_ctx(logged_in: bool) -> tera::Context {
    let mut ctx = tera::Context::new();
    ctx.insert("logged_in", &logged_in);
    ctx
}

fn fmt_date(t: time::OffsetDateTime) -> String {
    let fmt = time::macros::format_description!("[year]-[month]-[day]");
    t.format(&fmt).unwrap_or_default()
}

fn error_page(state: &AppState, status: StatusCode, message: &str) -> Response {
    let mut ctx = base_ctx(false);
    ctx.insert("status", &status.as_u16().to_string());
    ctx.insert("message", message);
    match state.tera.render("error.html", &ctx) {
        Ok(html) => (status, Html(html)).into_response(),
        Err(_) => (status, format!("{status}: {message}")).into_response(),
    }
}

/// Public interactive API reference (Swagger UI) for the embedded OpenAPI spec.
pub async fn api_docs(State(state): State<AppState>) -> Response {
    let ctx = base_ctx(false);
    page_response(&state, "api_docs.html", &ctx)
}

fn is_valid_slug(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn page_response(state: &AppState, template: &str, ctx: &tera::Context) -> Response {
    match render(state, template, ctx) {
        Ok(html) => html.into_response(),
        Err(e) => {
            tracing::error!("template render failed: {e}");
            error_page(
                state,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong.",
            )
        }
    }
}

/// Landing page.
pub async fn index(State(state): State<AppState>, MaybeUser(user): MaybeUser) -> Response {
    let ctx = base_ctx(user.is_some());
    page_response(&state, "index.html", &ctx)
}

/// Editor for a new post (works for anonymous visitors).
pub async fn editor_new(State(state): State<AppState>, MaybeUser(user): MaybeUser) -> Response {
    let mut ctx = base_ctx(user.is_some());
    ctx.insert("page_path", "");
    ctx.insert("page_title", "");
    ctx.insert("page_content", "");
    ctx.insert("page_format", "markdown");
    ctx.insert(
        "author_name",
        &user
            .as_ref()
            .and_then(|a| a.author_name.clone())
            .unwrap_or_default(),
    );
    ctx.insert(
        "author_url",
        &user
            .as_ref()
            .and_then(|a| a.author_url.clone())
            .unwrap_or_default(),
    );
    page_response(&state, "editor.html", &ctx)
}

/// Editor for an existing post (owner only).
pub async fn editor_edit(
    State(state): State<AppState>,
    RequireAuth(auth): RequireAuth,
    Path(path): Path<String>,
) -> Response {
    let page = match fetch_page(&state.pool, &path).await {
        Ok(Some(p)) => p,
        Ok(None) => return error_page(&state, StatusCode::NOT_FOUND, "Page not found."),
        Err(_) => {
            return error_page(
                &state,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong.",
            )
        }
    };
    if page.account_id != auth.id {
        return error_page(
            &state,
            StatusCode::FORBIDDEN,
            "You can only edit your own posts.",
        );
    }
    let mut ctx = base_ctx(true);
    ctx.insert("page_path", &page.path);
    ctx.insert("page_title", &page.title);
    ctx.insert("page_content", &page.content);
    ctx.insert("page_format", page.content_format.as_str());
    ctx.insert("author_name", &page.author_name.clone().unwrap_or_default());
    ctx.insert("author_url", &page.author_url.clone().unwrap_or_default());
    page_response(&state, "editor.html", &ctx)
}

/// Public page view; increments the view counter.
pub async fn view_page(State(state): State<AppState>, Path(path): Path<String>) -> Response {
    if !is_valid_slug(&path) {
        return error_page(&state, StatusCode::NOT_FOUND, "Page not found.");
    }
    let page = match fetch_page(&state.pool, &path).await {
        Ok(Some(p)) => p,
        Ok(None) => return error_page(&state, StatusCode::NOT_FOUND, "Page not found."),
        Err(_) => {
            return error_page(
                &state,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong.",
            )
        }
    };
    let html = page
        .html_cache
        .clone()
        .unwrap_or_else(|| crate::render::render_content(&page.content_format, &page.content));
    // Best-effort view counting; failures are logged but do not block the render.
    if let Err(e) = sqlx::query("UPDATE pages SET views = views + 1 WHERE path = $1")
        .bind(&path)
        .execute(&state.pool)
        .await
    {
        tracing::warn!("failed to increment views for {path}: {e}");
    }
    let mut ctx = base_ctx(false);
    ctx.insert("title", &page.title);
    ctx.insert("author_name", page.author_name.as_deref().unwrap_or(""));
    ctx.insert("author_url", page.author_url.as_deref().unwrap_or(""));
    ctx.insert("created", &fmt_date(page.created_at));
    ctx.insert("views", &page.views);
    ctx.insert("page_html", &html);
    page_response(&state, "page.html", &ctx)
}

#[derive(Debug, serde::Deserialize)]
pub struct LoginForm {
    pub token: String,
}

/// Token re-entry form.
pub async fn login_form(State(state): State<AppState>, MaybeUser(user): MaybeUser) -> Response {
    let mut ctx = base_ctx(user.is_some());
    ctx.insert("error", "");
    page_response(&state, "login.html", &ctx)
}

/// Process token re-entry: validate, set cookie, redirect to "My posts".
pub async fn login_submit(State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    let invalid = |state: &AppState| -> Response {
        let mut ctx = base_ctx(false);
        ctx.insert("error", "Invalid access token.");
        let status = StatusCode::UNAUTHORIZED;
        match state.tera.render("login.html", &ctx) {
            Ok(html) => (status, Html(html)).into_response(),
            Err(_) => (status, "Invalid access token.").into_response(),
        }
    };

    let Ok(id) = Uuid::parse_str(form.token.trim()) else {
        return invalid(&state);
    };
    match auth::fetch_account(&state.pool, id).await {
        Ok(Some(_)) => {
            let cookie = auth::token_cookie(&id, state.config.base_url.starts_with("https"));
            (
                [(axum::http::header::SET_COOKIE, cookie)],
                Redirect::to("/my"),
            )
                .into_response()
        }
        _ => invalid(&state),
    }
}

/// Log out: clear the cookie (the token itself remains valid).
pub async fn logout() -> Response {
    (
        [(axum::http::header::SET_COOKIE, clear_token_cookie())],
        Redirect::to("/"),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
struct PageRow {
    path: String,
    title: String,
    views: i64,
    created: String,
    updated: String,
}

/// The logged-in user's page list.
pub async fn my_posts(State(state): State<AppState>, RequireAuth(auth): RequireAuth) -> Response {
    let rows: Vec<PageSummary> = match sqlx::query_as(
        "SELECT path, title, views, created_at, updated_at FROM pages \
         WHERE account_id = $1 ORDER BY created_at DESC LIMIT 200",
    )
    .bind(auth.id)
    .fetch_all(&state.pool)
    .await
    {
        Ok(r) => r,
        Err(_) => {
            return error_page(
                &state,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong.",
            )
        }
    };
    let pages: Vec<PageRow> = rows
        .into_iter()
        .map(|p| PageRow {
            path: p.path,
            title: p.title,
            views: p.views,
            created: fmt_date(p.created_at),
            updated: fmt_date(p.updated_at),
        })
        .collect();
    let mut ctx = base_ctx(true);
    ctx.insert("pages", &pages);
    page_response(&state, "my_posts.html", &ctx)
}

/// Account settings form (also displays the access token).
pub async fn settings_form(
    State(state): State<AppState>,
    RequireAuth(auth): RequireAuth,
) -> Response {
    let mut ctx = base_ctx(true);
    ctx.insert("short_name", &auth.short_name);
    ctx.insert("author_name", auth.author_name.as_deref().unwrap_or(""));
    ctx.insert("author_url", auth.author_url.as_deref().unwrap_or(""));
    ctx.insert("token", &auth.id.to_string());
    page_response(&state, "settings.html", &ctx)
}

#[derive(Debug, serde::Deserialize)]
pub struct SettingsForm {
    pub short_name: String,
    pub author_name: String,
    pub author_url: String,
}

fn clean_form_field(s: String) -> Option<String> {
    let t = s.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

/// Save account settings.
pub async fn settings_submit(
    State(state): State<AppState>,
    RequireAuth(auth): RequireAuth,
    Form(form): Form<SettingsForm>,
) -> Response {
    let short_name = form.short_name.trim().to_string();
    if short_name.is_empty() || short_name.chars().count() > 64 {
        return error_page(
            &state,
            StatusCode::BAD_REQUEST,
            "Short name must be 1-64 characters.",
        );
    }
    let result = sqlx::query(
        "UPDATE accounts SET short_name = $2, author_name = $3, author_url = $4, \
         updated_at = now() WHERE id = $1",
    )
    .bind(auth.id)
    .bind(short_name)
    .bind(clean_form_field(form.author_name))
    .bind(clean_form_field(form.author_url))
    .execute(&state.pool)
    .await;
    match result {
        Ok(_) => Redirect::to("/settings").into_response(),
        Err(_) => error_page(
            &state,
            StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong.",
        ),
    }
}
