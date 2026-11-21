//! Environment-variable configuration.

use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection string (required).
    pub database_url: String,
    /// Address the HTTP server binds to. Defaults to `0.0.0.0:3000`.
    pub bind_addr: SocketAddr,
    /// Public base URL used when building page URLs in API responses.
    pub base_url: String,
    /// Directory containing the Tera `*.html` templates. Defaults to the
    /// crate's template directory (compile-time path); override for
    /// deployments (e.g. Docker) where templates live elsewhere.
    pub template_dir: String,
    /// Directory with static assets served under `/static`.
    pub static_dir: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable is required"))?;

        let bind_addr: SocketAddr = std::env::var("BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid BIND_ADDR: {e}"))?;

        let base_url = std::env::var("BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3000".to_string())
            .trim_end_matches('/')
            .to_string();

        let template_dir = std::env::var("TEMPLATE_DIR").unwrap_or_else(|_| {
            concat!(env!("CARGO_MANIFEST_DIR"), "/src/web/templates").to_string()
        });
        let static_dir = std::env::var("STATIC_DIR")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/static").to_string());

        Ok(Self {
            database_url,
            bind_addr,
            base_url,
            template_dir,
            static_dir,
        })
    }
}
