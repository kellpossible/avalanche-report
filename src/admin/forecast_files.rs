use axum::{
    response::{Redirect, Response},
    routing::get,
    Extension, Router,
};
use serde::Serialize;

use crate::{
    error::{map_eyre_error, map_std_error},
    state::AppState,
    templates::TemplatesWithContext,
    types,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index_handler))
        .route("/clear", get(clear_handler))
}

#[derive(Serialize)]
struct ForecastFileDetails {
    google_drive_id: String,
    time: Option<types::Time>,
}

#[derive(Serialize)]
struct Context {
    forecast_files: Vec<ForecastFileDetails>,
}

pub async fn index_handler(
    Extension(templates): Extension<TemplatesWithContext>,
    Extension(database): Extension<crate::database::Database>,
) -> axum::response::Result<Response> {
    let forecast_files = sqlx::query!(r#"SELECT google_drive_id, json_extract(parsed_forecast, "$.time") as time FROM forecast_files"#)
    .try_map(|record| {
        Ok(ForecastFileDetails {
            google_drive_id: record.google_drive_id,
            time: Option::transpose(record.time.map(|t| t.parse::<types::Time>())).map_err(|e| sqlx::Error::Decode(e.into()))?,
        })
    }).fetch_all(&database).await.map_err(map_std_error)?;
    let context = Context { forecast_files };
    templates
        .render("admin/forecast_files.html", &context)
        .map_err(map_eyre_error)
        .map_err(Into::into)
}

pub async fn clear_handler(
    Extension(database): Extension<crate::database::Database>,
) -> axum::response::Result<Redirect> {
    sqlx::query!("DELETE FROM forecast_files")
        .execute(&database)
        .await
        .map_err(map_std_error)?;
    Ok(Redirect::to("../forecast-files"))
}
