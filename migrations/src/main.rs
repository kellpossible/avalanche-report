use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use toml_env::AutoMapEnvArgs;

pub const DB_FILE_NAME: &str = "db.sqlite3";

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Directory where application data is stored (including logs).
    ///
    /// Default is `data`.
    pub data_dir: PathBuf,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
        }
    }
}

impl Options {
    /// Initialize options using the [`toml_env`] library.
    pub async fn initialize() -> eyre::Result<Options> {
        Ok(toml_env::initialize(toml_env::Args {
            config_variable_name: "AVALANCHE_REPORT",
            logging: toml_env::Logging::StdOut,
            auto_map_env: Some(AutoMapEnvArgs {
                prefix: Some("AVALANCHE_REPORT"),
                ..AutoMapEnvArgs::default()
            }),
            ..toml_env::Args::default()
        })?
        .unwrap_or_default())
    }
}

fn default_data_dir() -> PathBuf {
    "data".into()
}

#[tokio::main]
pub async fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt().init();
    color_eyre::install()?;
    let options = Options::initialize().await?;

    let path = options.data_dir.join(DB_FILE_NAME);
    if path.exists() {
        tracing::info!("Using existing database: {path:?}");
    } else {
        tracing::info!("No existing database found, initializing new one: {path:?}");
        if !options.data_dir.exists() {
            std::fs::create_dir_all(&options.data_dir)?;
        }
    }

    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true),
    )
    .await?;

    tracing::info!("Running migrations.");
    migrations::run(&pool).await?;
    Ok(())
}
