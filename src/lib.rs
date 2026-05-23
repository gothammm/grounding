pub mod api;
pub mod config;
pub mod store;

pub use config::Config;
pub use store::Store;

pub async fn serve(data_dir: &str, port: u16) -> anyhow::Result<()> {
    let config = Config::new(data_dir);
    let store = Store::open(&config)?;
    api::serve(store, port).await
}
