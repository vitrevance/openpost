//! REST JSON API under `/api`.

pub mod accounts;
pub mod dto;
pub mod pages;
pub mod preview;

use axum::http::{header, HeaderMap, HeaderValue};
use axum::routing::get;
use axum::Router;

use crate::state::AppState;

/// Embedded OpenAPI 3.0.3 description of this API.
const OPENAPI_SPEC: &str = include_str!("openapi.json");

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/ping", get(ping))
        .route(
            "/api/account",
            get(accounts::get_account)
                .post(accounts::create_account)
                .put(accounts::update_account),
        )
        .route("/api/auth/login", axum::routing::post(accounts::login))
        .route(
            "/api/pages",
            get(pages::list_pages).post(pages::create_page),
        )
        .route(
            "/api/pages/{path}",
            get(pages::get_page)
                .put(pages::edit_page)
                .delete(pages::delete_page),
        )
        .route("/api/pages/{path}/views", get(pages::page_views))
        .route("/api/preview", axum::routing::post(preview::preview))
        .route("/api/openapi.json", get(openapi_json))
}

async fn openapi_json() -> (HeaderMap, &'static str) {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    (headers, OPENAPI_SPEC)
}

async fn ping() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "ok": true }))
}
