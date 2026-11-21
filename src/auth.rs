//! Token-based authentication.
//!
//! The server-generated UUID account id IS the access token. It is resolved
//! from an `Authorization: Bearer <uuid>` header (API clients) or the
//! `op_token` HttpOnly cookie (browser sessions).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::Account;
use crate::state::AppState;

pub const TOKEN_COOKIE: &str = "op_token";
/// 10 years, in seconds.
const TOKEN_COOKIE_MAX_AGE: u64 = 10 * 365 * 24 * 60 * 60;

/// Extract the access token UUID from request headers.
///
/// Order of precedence: `Authorization: Bearer <uuid>` header, then the
/// `op_token` cookie. Returns `None` when absent or malformed.
pub fn extract_token(parts: &Parts) -> Option<Uuid> {
    if let Some(auth) = parts.headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(value) = auth.to_str() {
            if let Some(token) = value.strip_prefix("Bearer ") {
                return token.trim().parse().ok();
            }
        }
    }
    if let Some(cookie) = parts.headers.get(axum::http::header::COOKIE) {
        if let Ok(value) = cookie.to_str() {
            for pair in value.split(';') {
                if let Some(token) = pair.trim().strip_prefix(concat!("op_token", "=")) {
                    return token.trim().parse().ok();
                }
            }
        }
    }
    None
}

/// Fetch an account by its id (= token). `Ok(None)` when unknown.
pub async fn fetch_account(pool: &sqlx::PgPool, id: Uuid) -> Result<Option<Account>, AppError> {
    sqlx::query_as::<_, Account>(
        "SELECT id, short_name, author_name, author_url, created_at \
         FROM accounts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)
}

/// Extractor that resolves the current user without failing the request
/// (used by public pages and the editor, which work for anonymous visitors).
pub struct MaybeUser(pub Option<Account>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = match extract_token(parts) {
            Some(id) => fetch_account(&state.pool, id).await.ok().flatten(),
            None => None,
        };
        Ok(MaybeUser(user))
    }
}

/// Extractor that requires a valid token; produces 401 otherwise.
pub struct RequireAuth(pub Account);

impl FromRequestParts<AppState> for RequireAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let id = extract_token(parts).ok_or(AppError::Unauthorized)?;
        let account = fetch_account(&state.pool, id)
            .await?
            .ok_or(AppError::Unauthorized)?;
        Ok(RequireAuth(account))
    }
}

/// Build a `Set-Cookie` header value that persists the token for ~10 years.
pub fn token_cookie(token: &Uuid, secure: bool) -> String {
    let mut cookie = format!(
        "{TOKEN_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={TOKEN_COOKIE_MAX_AGE}"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

/// Build a `Set-Cookie` header value that clears the token cookie.
pub fn clear_token_cookie() -> String {
    format!("{TOKEN_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    const UUID_STR: &str = "0f14d0ab-9605-4a62-a9e4-5ed26688389b";

    fn parts(auth_header: Option<&str>, cookie_header: Option<&str>) -> Parts {
        let mut builder = Request::builder();
        if let Some(h) = auth_header {
            builder = builder.header("authorization", h);
        }
        if let Some(c) = cookie_header {
            builder = builder.header("cookie", c);
        }
        let req = builder.body(()).unwrap();
        let (parts, _) = req.into_parts();
        parts
    }

    #[test]
    fn extracts_bearer_token() {
        let p = parts(Some(&format!("Bearer {UUID_STR}")), None);
        assert_eq!(extract_token(&p).unwrap().to_string(), UUID_STR);
    }

    #[test]
    fn rejects_malformed_bearer() {
        assert!(extract_token(&parts(Some("Bearer not-a-uuid"), None)).is_none());
        assert!(extract_token(&parts(Some("Basic abcdef"), None)).is_none());
        assert!(extract_token(&parts(Some("no-scheme"), None)).is_none());
    }

    #[test]
    fn extracts_cookie_token() {
        let p = parts(None, Some(&format!("other=1; op_token={UUID_STR}; x=y")));
        assert_eq!(extract_token(&p).unwrap().to_string(), UUID_STR);
    }

    #[test]
    fn cookie_prefix_must_match_exactly() {
        // "xop_token=" must not match.
        assert!(extract_token(&parts(None, Some(&format!("xop_token={UUID_STR}")))).is_none());
    }

    #[test]
    fn no_token_returns_none() {
        assert!(extract_token(&parts(None, None)).is_none());
    }
}
