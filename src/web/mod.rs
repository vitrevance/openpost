//! Server-rendered web UI routes.

pub mod views;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(views::index))
        .route("/write", get(views::editor_new))
        .route("/edit/{path}", get(views::editor_edit))
        .route("/login", get(views::login_form).post(views::login_submit))
        .route("/logout", post(views::logout))
        .route("/my", get(views::my_posts))
        .route(
            "/settings",
            get(views::settings_form).post(views::settings_submit),
        )
        .route("/api-docs", get(views::api_docs))
        // Catch-all for public pages (8-char base62 slugs). Concrete routes
        // above take precedence over this dynamic route.
        .route("/{path}", get(views::view_page))
}
