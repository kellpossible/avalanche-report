use deadpool_sqlite::{Config, Pool, Runtime};
use std::path::Path;
use std::sync::Arc;

pub async fn initialize(data_dir: &Path) -> eyre::Result<Database> {
    let path = data_dir.join("db.sqlite3");
    if path.exists() {
        tracing::info!("Using existing database: {path:?}");
    } else {
        tracing::info!("No existing database found, initializing new one: {path:?}");
    }
    let config = Config::new(path);
    let pool = config
        .create_pool(Runtime::Tokio1)
        .map_err(eyre::Error::from)?;

    drop(pool.get().await?);

    Ok(Database {
        pool: Arc::new(pool),
    })
}

#[derive(Clone)]
pub struct Database {
    pool: Arc<Pool>,
}
