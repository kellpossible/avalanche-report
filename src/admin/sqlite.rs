//! An Sqlite query interface for debugging.

use axum::{
    response::Response,
    routing::{get, post},
    Extension, Form, Router,
};
use eyre::Context as _;
use serde::{Deserialize, Serialize};
use sqlx::{Column, Executor, Row};

use crate::{
    database::Database, error::map_eyre_error, state::AppState, templates::TemplatesWithContext,
    types,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(post_handler))
        .route("/", get(get_handler))
}

#[derive(Debug, Serialize)]
struct Context {
    query: String,
    result: String,
}

async fn get_handler(
    Extension(templates): Extension<TemplatesWithContext>,
) -> axum::response::Result<Response> {
    let context = Context {
        query: "".to_string(),
        result: "".to_string(),
    };
    templates
        .render("admin/sqlite.html", &context)
        .map_err(map_eyre_error)
        .map_err(Into::into)
}

#[derive(Debug, Deserialize)]
struct FormData {
    query: String,
}

async fn post_handler(
    Extension(database): Extension<Database>,
    Extension(templates): Extension<TemplatesWithContext>,
    Form(form): Form<FormData>,
) -> axum::response::Result<Response> {
    let query = form.query;
    tracing::info!("Performing admin query: {query}");
    let mut conn = database
        .acquire()
        .await
        .context("Error acquiring database connection")
        .map_err(map_eyre_error)?;
    let result: String = match conn
        .fetch_all(query.as_str())
        .await
        .context("Error performing query")
    {
        Ok(rows) => rows
            .iter()
            .map(|row| {
                Ok(row
                    .columns()
                    .iter()
                    .map(|column| {
                        let value: types::AnyValue = row.get_unchecked(column.ordinal());
                        Ok(format!("{value:?}"))
                    })
                    .collect::<eyre::Result<Vec<String>>>()
                    .context("Error collecting column values")?
                    .join(", "))
            })
            .collect::<eyre::Result<Vec<String>>>()
            .map_err(map_eyre_error)?
            .join("\n"),
        Err(error) => {
            tracing::error!("Error performing query: {:#}", error);
            format!("{:#}", error)
        }
    };
    let context = Context { query, result };
    templates
        .render("admin/sqlite.html", &context)
        .map_err(map_eyre_error)
        .map_err(Into::into)
}
