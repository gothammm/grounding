pub mod api;
pub mod config;
pub mod mcp;
pub mod store;

pub use config::Config;
pub use store::Store;

pub async fn serve(data_dir: &str, port: u16) -> anyhow::Result<()> {
    let config = Config::new(data_dir);
    let store = Store::open(&config)?;
    api::serve(store, port).await
}

pub async fn serve_mcp_stdio(data_dir: &str) -> anyhow::Result<()> {
    let config = Config::new(data_dir);
    let store = Store::open(&config)?;
    let handler = mcp::McpHandler::new(store);
    tracing::info!("MCP stdio server started");
    handler.serve_stdio().await
}
