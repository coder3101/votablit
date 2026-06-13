use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::net::SocketAddr;
use tracing::info;

use votablit::db;
use votablit::state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "votablit=info,tower_http=info".into()),
        )
        .init();

    let db_path = std::env::var("DATABASE_PATH")
        .unwrap_or_else(|_| "leaderboard.db".to_string());

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .expect("Failed to connect to database");

    db::init_db(&pool)
        .await
        .expect("Failed to initialize database");

    let admin_token = std::env::var("ADMIN_TOKEN")
        .expect("ADMIN_TOKEN must be set");
    assert!(!admin_token.is_empty(), "ADMIN_TOKEN must not be empty");

    let state = AppState::new(pool, admin_token);
    let listener = bind_listener().await;
    let addr = listener.local_addr().unwrap();

    info!("Server listening on {addr}");

    axum::serve(
        listener,
        votablit::app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("Server error");
}

/// Bind to the address specified by `BIND_ADDR` or default to `0.0.0.0:8080`.
async fn bind_listener() -> tokio::net::TcpListener {
    let bind_addr =
        std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("Failed to bind to address")
}

/// Wait for SIGTERM (Fly.io) or Ctrl+C (local dev).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, starting graceful shutdown");
}
