//! openpost — a clean-room telegra.ph clone with UUID token auth.

mod api;
mod auth;
mod config;
mod db;
mod error;
mod models;
mod render;
mod slug;
mod state;
mod web;

use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = config::Config::from_env()?;
    let pool = db::init_pool(&config.database_url).await?;
    db::run_migrations(&pool).await?;
    tracing::info!("database migrations applied");

    let tera = tera::Tera::new(&format!(
        "{}/**/*.html",
        config.template_dir.trim_end_matches('/')
    ))?;

    let state = AppState {
        pool,
        tera,
        config: config.clone(),
    };

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("openpost listening on http://{}", listener.local_addr()?);

    let app = axum::Router::new()
        .merge(web::router())
        .merge(api::router())
        .nest_service("/static", ServeDir::new(state.config.static_dir.clone()))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        tokio::select! {
            _ = sigterm.recv() => {},
            _ = sigint.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install SIGINT handler");
    }
    tracing::info!("shutdown signal received");
}
