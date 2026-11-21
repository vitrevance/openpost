//! Uniform error type mapped to HTTP responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

/// Application error type. Converted into a JSON `{"error": "..."}` body.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),

    #[error("unauthorized: missing or invalid access token")]
    Unauthorized,

    #[error("not found")]
    NotFound,

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(anyhow::anyhow!(e))
    }
}

#[derive(serde::Serialize)]
pub struct ApiErrorBody {
    pub error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized: missing or invalid access token".to_string(),
            ),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            Self::Internal(ref e) => {
                // Log the full detail server-side; never leak internals to clients.
                tracing::error!("internal error: {e:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };
        (status, Json(ApiErrorBody { error: message })).into_response()
    }
}
