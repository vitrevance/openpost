//! `/api/account` and `/api/auth/login` handlers.

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use uuid::Uuid;

use crate::auth::{self, RequireAuth};
use crate::error::AppError;
use crate::models::Account;
use crate::state::AppState;

use super::dto::{
    clean_option, AccountResponse, CreateAccountRequest, LoginRequest, UpdateAccountRequest,
    MAX_SHORT_NAME,
};

/// Create a new account. The server generates a UUIDv4 which is both the
/// account id and the access token.
pub async fn create_account(
    State(state): State<AppState>,
    Json(req): Json<CreateAccountRequest>,
) -> Result<(axum::http::StatusCode, Json<AccountResponse>), AppError> {
    let short_name = req.short_name.trim().to_string();
    if short_name.is_empty() || short_name.chars().count() > MAX_SHORT_NAME {
        return Err(AppError::BadRequest(format!(
            "short_name must be 1-{MAX_SHORT_NAME} characters"
        )));
    }
    let account = insert_account(
        &state.pool,
        Uuid::new_v4(),
        &short_name,
        clean_option(req.author_name),
        clean_option(req.author_url),
    )
    .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(AccountResponse::from(account)),
    ))
}

/// Shared INSERT used by `create_account` and the anonymous-account path in
/// `pages::create_page`.
pub async fn insert_account(
    pool: &sqlx::PgPool,
    id: Uuid,
    short_name: &str,
    author_name: Option<String>,
    author_url: Option<String>,
) -> Result<Account, AppError> {
    let account = sqlx::query_as::<_, Account>(
        "INSERT INTO accounts (id, short_name, author_name, author_url) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, short_name, author_name, author_url, created_at",
    )
    .bind(id)
    .bind(short_name)
    .bind(author_name)
    .bind(author_url)
    .fetch_one(pool)
    .await?;
    Ok(account)
}

/// Return the authenticated account.
pub async fn get_account(
    RequireAuth(auth): RequireAuth,
) -> Result<Json<AccountResponse>, AppError> {
    Ok(Json(AccountResponse::from(auth)))
}

/// Partially update the authenticated account.
pub async fn update_account(
    State(state): State<AppState>,
    RequireAuth(auth): RequireAuth,
    Json(req): Json<UpdateAccountRequest>,
) -> Result<Json<AccountResponse>, AppError> {
    let short_name = req
        .short_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| auth.short_name.clone());
    if short_name.chars().count() > MAX_SHORT_NAME {
        return Err(AppError::BadRequest(format!(
            "short_name must be 1-{MAX_SHORT_NAME} characters"
        )));
    }
    // Omitted optional fields keep their current value; a provided field
    // (even an empty string, which clears it) replaces it.
    let author_name = match req.author_name {
        Some(s) => clean_option(Some(s)),
        None => auth.author_name.clone(),
    };
    let author_url = match req.author_url {
        Some(s) => clean_option(Some(s)),
        None => auth.author_url.clone(),
    };
    let account = sqlx::query_as::<_, Account>(
        "UPDATE accounts SET short_name = $2, author_name = $3, author_url = $4, \
         updated_at = now() WHERE id = $1 \
         RETURNING id, short_name, author_name, author_url, created_at",
    )
    .bind(auth.id)
    .bind(short_name)
    .bind(author_name)
    .bind(author_url)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(AccountResponse::from(account)))
}

/// Validate an access token; on success also (re-)set the browser cookie.
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, AppError> {
    let id = Uuid::parse_str(req.token.trim()).map_err(|_| AppError::Unauthorized)?;
    let account = auth::fetch_account(&state.pool, id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let cookie = auth::token_cookie(&id, state.config.base_url.starts_with("https"));
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(AccountResponse::from(account)),
    )
        .into_response())
}
