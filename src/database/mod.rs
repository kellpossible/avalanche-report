use axum::extract::State;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use deadpool_sqlite::PoolError;
use http::{Request, StatusCode};
use nonzero_ext::nonzero;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use time::format_description::well_known::iso8601::TimePrecision;
use time::format_description::well_known::{iso8601, Iso8601};

use crate::state::AppState;

pub mod backup;
pub mod blob;
mod migrations;

// pub const DATETIME_FORMAT: &[FormatItem<'static>] =
//     format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]");

pub const DATETIME_CONFIG: iso8601::EncodedConfig = iso8601::Config::DEFAULT
    .set_time_precision(TimePrecision::Second {
        decimal_digits: Some(nonzero!(3u8)),
    })
    .encode();
pub const DATETIME_FORMAT: Iso8601<DATETIME_CONFIG> = Iso8601;
pub const DB_FILE_NAME: &str = "db.sqlite3";

pub async fn initialize(data_dir: &Path) -> eyre::Result<Database> {
    let path = data_dir.join(DB_FILE_NAME);
    if path.exists() {
        tracing::info!("Using existing database: {path:?}");
    } else {
        tracing::info!("No existing database found, initializing new one: {path:?}");
    }

    let config = deadpool_sqlite::Config::new(path);
    let pool = config.create_pool(deadpool_sqlite::Runtime::Tokio1)?;

    pool.get()
        .await?
        .interact(|conn| migrations::run(conn))
        .await
        .map_err(|err| eyre::eyre!("{err}"))??;

    Ok(Database {
        pool: Arc::new(pool),
    })
}

#[derive(Clone)]
pub struct Database {
    pool: Arc<deadpool_sqlite::Pool>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Pool(#[from] deadpool_sqlite::PoolError),
    #[error("Error while interacting with the database")]
    Interact(String),
}

impl From<deadpool_sqlite::InteractError> for Error {
    fn from(error: deadpool_sqlite::InteractError) -> Self {
        Self::Interact(error.to_string())
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::Pool(PoolError::Timeout(_)) => {
                (StatusCode::SERVICE_UNAVAILABLE, self.to_string())
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        }
        .into_response()
    }
}

impl Database {
    /// See [Pool::get()].
    pub async fn get(&self) -> Result<DatabaseInstance, Error> {
        Ok(DatabaseInstance(Arc::new(self.pool.get().await?)))
    }
}

#[derive(Clone)]
pub struct DatabaseInstance(Arc<deadpool_sqlite::Object>);

impl DatabaseInstance {
    pub async fn interact<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&mut rusqlite::Connection) -> R + Send + 'static,
        R: Send + 'static,
    {
        self.0.interact(f).await.map_err(Error::from)
    }
}

#[tracing::instrument(skip_all)]
pub async fn middleware<B>(
    state: State<AppState>,
    mut request: Request<B>,
    next: Next<B>,
) -> Response {
    let database = match state.database.get().await {
        Ok(database) => database,
        Err(error) => {
            return error.into_response();
        }
    };
    request.extensions_mut().insert(database);
    next.run(request).await
}
