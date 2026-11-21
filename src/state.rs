//! Shared application state passed to all handlers.

use sqlx::PgPool;
use tera::Tera;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub tera: Tera,
    pub config: Config,
}
