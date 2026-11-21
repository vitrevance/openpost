//! Random base62 slugs for public page URLs (e.g. `a1B2c3D4`).

use rand::distr::{Alphanumeric, SampleString};
use sqlx::PgPool;

use crate::error::AppError;

/// Generate a random base62 string of `len` characters.
pub fn generate_slug(len: usize) -> String {
    Alphanumeric.sample_string(&mut rand::rng(), len)
}

/// Generate an 8-character slug that does not collide with an existing page.
///
/// 8 base62 characters give ~2.1e14 combinations, so collisions are rare;
/// we retry a bounded number of times before giving up.
pub async fn unique_slug(pool: &PgPool) -> Result<String, AppError> {
    for _ in 0..10 {
        let slug = generate_slug(8);
        let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM pages WHERE path = $1")
            .bind(&slug)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            return Ok(slug);
        }
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "could not generate a unique page slug"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_8_alphanumeric_chars() {
        for _ in 0..100 {
            let s = generate_slug(8);
            assert_eq!(s.len(), 8);
            assert!(s.bytes().all(|b| b.is_ascii_alphanumeric()));
        }
    }

    #[test]
    fn slugs_are_random() {
        // With 62^8 combinations, 100 draws should not all be identical.
        let first = generate_slug(8);
        assert!(
            generate_slug(8) != first || generate_slug(8) != first || generate_slug(8) != first
        );
    }
}
