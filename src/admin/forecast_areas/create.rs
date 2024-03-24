use axum::{
    response::{IntoResponse, Response},
    Extension,
};
use eyre::ContextCompat;

use crate::{
    error::{map_eyre_error, map_std_error},
    forecast_areas::{ForecastArea, ForecastAreaId},
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
    Extension(database): Extension<crate::database::Database>,
    multipart: axum::extract::Multipart,
) -> axum::response::Result<()> {
    post(multipart).await.map_err(map_eyre_error)
}

pub async fn post(mut multipart: axum::extract::Multipart) -> eyre::Result<()> {
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
    tracing::info!("Created forecast area: {forecast_area:?}");

    Ok(())
}
