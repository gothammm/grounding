mod handlers;
pub mod models;

use std::sync::Arc;

use axum::{routing::{get, post}, Router};

use crate::mcp::McpHandler;
use crate::store::Store;

pub use handlers::AppState;

pub async fn serve(store: Store, port: u16) -> anyhow::Result<()> {
    let mcp_handler = McpHandler::new(store.clone());

    let state = AppState { store: Arc::new(store) };

    let app = Router::new()
        .route("/health", get(handlers::health))
        .route("/query", post(handlers::query_handler))
        .route("/index", post(handlers::index_handler))
        .route("/index/batch", post(handlers::batch_index_handler))
        .route("/documents", post(handlers::get_document_handler))
        .route("/metrics", get(handlers::metrics_handler))
        .with_state(state);

    let app = app.nest_service("/mcp", mcp_handler.http_service());

    let addr = format!("0.0.0.0:{port}");
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { tracing::info!("received Ctrl+C, shutting down"); }
        _ = terminate => { tracing::info!("received SIGTERM, shutting down"); }
    }
}
