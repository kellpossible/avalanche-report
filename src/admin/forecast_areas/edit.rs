use axum::{
    extract,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Extension, Router,
};
use eyre::ContextCompat;
use serde::{Deserialize, Serialize};

use crate::{
    database::Database,
    error::map_eyre_error,
    forecast_areas::{upsert_forecast_area, ForecastArea, ForecastAreaId},
    state::AppState,
    templates::TemplatesWithContext,
};

#[derive(Deserialize)]
struct PathParameters {
    forecast_area_id: ForecastAreaId,
}

#[derive(Serialize)]
struct Context {
    forecast_area_id: ForecastAreaId,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_handler))
        .route("/", post(post_handler))
}

async fn get_handler(
    extract::Path(path): extract::Path<PathParameters>,
    Extension(templates): Extension<TemplatesWithContext>,
) -> axum::response::Result<Response> {
    let context = Context {
        forecast_area_id: path.forecast_area_id,
    };
    Ok(templates
        .render("admin/forecast_areas/edit.html", &context)
        .map_err(map_eyre_error)?)
}

async fn post_handler(
    extract::Path(path): extract::Path<PathParameters>,
    Extension(database): Extension<Database>,
    multipart: axum::extract::Multipart,
) -> axum::response::Result<Response> {
    post_impl(path.forecast_area_id, &database, multipart)
        .await
        .map_err(map_eyre_error)?;
    Ok(Redirect::to("../../forecast-areas").into_response())
}

pub async fn post_impl(
    id: ForecastAreaId,
    database: &Database,
    mut multipart: axum::extract::Multipart,
) -> eyre::Result<()> {
    let mut geojson = None;
    while let Some(field) = multipart.next_field().await? {
        match field.name() {
            Some("geojson") => {
                geojson = Some(serde_json::from_slice(&field.bytes().await?)?);
            }
            _ => {}
        }
    }

    let forecast_area = ForecastArea {
        id,
        geojson: geojson.wrap_err("geojson field was not specified")?,
    };

    upsert_forecast_area(database, forecast_area).await?;

    Ok(())
}
