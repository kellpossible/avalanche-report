use deadpool_sqlite::{Config, Object, Pool, Runtime};
use eyre::Context;
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
    let pool = config.create_pool(Runtime::Tokio1)?;

    let migrations_runner = migrations::migrations::runner();
    {
        let db = pool.get().await?;
        tracing::info!("Running database migrations (if required).");
        tokio::task::block_in_place(|| {
            let mut conn = db.lock().unwrap();
            migrations_runner
                .run(&mut *conn)
                .wrap_err("Error while running database migrations")?;
            drop(conn);
            Ok::<(), eyre::Error>(())
        })?;
    }

    Ok(Database {
        pool: Arc::new(pool),
    })
}

#[derive(Clone)]
pub struct Database {
    pool: Arc<Pool>,
}

mod migrations {
    use refinery::embed_migrations;
    embed_migrations!("./src/migrations");
}

impl Database {
    /// See [Pool::get()].
    pub async fn get(&self) -> eyre::Result<Object> {
        Ok(self.pool.get().await?)
    }
}
