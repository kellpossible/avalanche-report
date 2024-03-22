use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use deadpool_sqlite::PoolError;
use http::StatusCode;
use nonzero_ext::nonzero;
use sqlx::Acquire;
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

    let pool =
        sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(path))
            .await?;

    let conn = pool.acquire().await?.acquire().await?;
    migrations::run(conn)?;

    pool.get()
        .awit?
        .interact(|conn| migrations::run(conn))
        .await
        .map_err(|err| eyre::eyre!("{err}"))??;

    Ok(pool)
}

pub type Database = sqlx::SqlitePool;

#[tracing::instrument(skip_all)]
pub async fn middleware(state: State<AppState>, mut request: Request, next: Next) -> Response {
    let database = match state.database.get().await {
        Ok(database) => database,
        Err(error) => {
            return error.into_response();
        }
    };
    request.extensions_mut().insert(database);
    next.run(request).await
}
