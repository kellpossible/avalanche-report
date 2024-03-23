use std::path::PathBuf;

use eyre::ContextCompat;
use serde::{Deserialize, Serialize};
use sqlx::Acquire;
use toml_env::AutoMapEnvArgs;

pub const DB_FILE_NAME: &str = "db.sqlite3";

#[derive(Serialize, Deserialize)]
pub struct Options {
    /// Directory where application data is stored (including logs).
    ///
    /// Default is `data`.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

impl Options {
    /// Initialize options using the [`toml_env`] library.
    pub async fn initialize() -> eyre::Result<Options> {
        toml_env::initialize(toml_env::Args {
            config_variable_name: "AVALANCHE_REPORT",
            logging: toml_env::Logging::StdOut,
            auto_map_env: Some(AutoMapEnvArgs {
                prefix: Some("AVALANCHE_REPORT"),
                ..AutoMapEnvArgs::default()
            }),
            ..toml_env::Args::default()
        })?
        .wrap_err("No configuration specified")
    }
}

fn default_data_dir() -> PathBuf {
    "data".into()
}

#[tokio::main]
pub async fn main() -> eyre::Result<()> {
    let options = Options::initialize().await?;
    let reporting_options: &'static axum_reporting::Options =
        Box::leak(Box::new(axum_reporting::Options {
            default_filter: "warn,migrations=info".to_owned(),
            page_title: "avalanche-report-migrations".to_owned(),
            data_dir: options.data_dir.clone(),
            log_rotation: tracing_appender::rolling::Rotation::DAILY,
            log_file_name: "avalanche-report-migrations".to_owned(),
        }));

    let _reporting_guard = axum_reporting::initialize(reporting_options)?;

    let path = options.data_dir.join(DB_FILE_NAME);
    if path.exists() {
        tracing::info!("Using existing database: {path:?}");
    } else {
        tracing::info!("No existing database found, initializing new one: {path:?}");
    }

    let pool =
        sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(path))
            .await?;

    tracing::info!("Running migrations.");
    migrations::run(&pool).await?;
    Ok(())
}
