use axum::{
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use eyre::ContextCompat;

use crate::{
    database::Database,
    error::{map_eyre_error, map_std_error},
    forecast_areas::{upsert_forecast_area, ForecastArea, ForecastAreaId},
    templates::TemplatesWithContext,
};

pub async fn get_handler(
    Extension(templates): Extension<TemplatesWithContext>,
) -> axum::response::Result<Response> {
    templates
        .render("admin/forecast_areas/create.html", &())
        .map_err(map_eyre_error)
}

pub async fn post_handler(
    Extension(database): Extension<Database>,
    multipart: axum::extract::Multipart,
) -> axum::response::Result<Response> {
    post(&database, multipart).await.map_err(map_eyre_error)?;
    Ok(Redirect::to("../forecast-areas").into_response())
}

pub async fn post(
    database: &Database,
    mut multipart: axum::extract::Multipart,
) -> eyre::Result<()> {
    let mut id = None;
    let mut geojson = None;
    while let Some(field) = multipart.next_field().await? {
        match field.name() {
            Some("id") => {
                id = Some(field.text().await?.into());
            }
            Some("geojson") => {
                geojson = Some(serde_json::from_slice(&field.bytes().await?)?);
            }
            _ => {}
        }
    }

    let forecast_area = ForecastArea {
        id: id.wrap_err("id field was not specified")?,
        geojson: geojson.wrap_err("geojson field was not specified")?,
    };

    upsert_forecast_area(database, forecast_area).await?;

    Ok(())
}
