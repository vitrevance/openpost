//! `/api/preview`: render Markdown/HTML to sanitized HTML for the editor's
//! preview pane (preview button and split-screen live preview).

use axum::extract::State;
use axum::Json;

use super::dto::{PreviewRequest, PreviewResponse, MAX_CONTENT};
use crate::error::AppError;
use crate::render::render_content;
use crate::state::AppState;

pub async fn preview(
    State(_state): State<AppState>,
    Json(req): Json<PreviewRequest>,
) -> Result<Json<PreviewResponse>, AppError> {
    if req.content.len() > MAX_CONTENT {
        return Err(AppError::BadRequest(format!(
            "content must be at most {MAX_CONTENT} bytes"
        )));
    }
    Ok(Json(PreviewResponse {
        html: render_content(&req.content_format, &req.content),
    }))
}
