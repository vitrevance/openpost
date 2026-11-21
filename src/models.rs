//! Domain models shared across the application.

use std::fmt;
use std::str::FromStr;

use time::OffsetDateTime;
use uuid::Uuid;

/// The content format of a page: Markdown source or raw HTML source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentFormat {
    #[default]
    Markdown,
    Html,
}

impl ContentFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Html => "html",
        }
    }
}

impl fmt::Display for ContentFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ContentFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "markdown" => Ok(Self::Markdown),
            "html" => Ok(Self::Html),
            other => Err(format!("unknown content format: {other}")),
        }
    }
}

// --- sqlx integration: store the enum as TEXT ('markdown' | 'html') ---

impl sqlx::Type<sqlx::Postgres> for ContentFormat {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for ContentFormat {
    fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<'r, sqlx::Postgres>>::decode(value)?;
        s.parse().map_err(Into::into)
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for ContentFormat {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        <&str as sqlx::Encode<'q, sqlx::Postgres>>::encode_by_ref(&self.as_str(), buf)
    }
}

/// An account. `id` is a server-generated UUIDv4 that doubles as the access
/// token — it is the ONLY credential, and it cannot be recovered if lost.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Account {
    pub id: Uuid,
    pub short_name: String,
    pub author_name: Option<String>,
    pub author_url: Option<String>,
    pub created_at: OffsetDateTime,
}

/// A published page.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Page {
    pub path: String,
    pub account_id: Uuid,
    pub title: String,
    pub content: String,
    pub content_format: ContentFormat,
    pub author_name: Option<String>,
    pub author_url: Option<String>,
    pub html_cache: Option<String>,
    pub views: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Lightweight page projection used for list endpoints.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PageSummary {
    pub path: String,
    pub title: String,
    pub views: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_format_serde_roundtrip() {
        let md: ContentFormat = serde_json::from_str("\"markdown\"").unwrap();
        assert_eq!(md, ContentFormat::Markdown);
        let html: ContentFormat = serde_json::from_str("\"html\"").unwrap();
        assert_eq!(html, ContentFormat::Html);
        assert!(serde_json::from_str::<ContentFormat>("\"text\"").is_err());
        assert_eq!(
            serde_json::to_string(&ContentFormat::Markdown).unwrap(),
            "\"markdown\""
        );
    }

    #[test]
    fn content_format_from_str() {
        assert_eq!(
            "markdown".parse::<ContentFormat>().unwrap(),
            ContentFormat::Markdown
        );
        assert_eq!(
            "html".parse::<ContentFormat>().unwrap(),
            ContentFormat::Html
        );
        assert!("bogus".parse::<ContentFormat>().is_err());
    }

    #[test]
    fn content_format_defaults_to_markdown() {
        assert_eq!(ContentFormat::default(), ContentFormat::Markdown);
    }
}
