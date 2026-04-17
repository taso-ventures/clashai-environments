//! Unified environment server entry point.

use std::net::SocketAddr;

use environment_server::{build_router, log_startup, AppState};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialise logging.
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "environment_server=info,warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let public_base_url =
        std::env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| format!("http://localhost:{port}"));

    let ws_capacity: usize = std::env::var("WS_CAPACITY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(256);

    let state = AppState::new(public_base_url, ws_capacity);
    let router = build_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log_startup(&addr.to_string());

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind listener");

    axum::serve(listener, router).await.expect("server error");
}
