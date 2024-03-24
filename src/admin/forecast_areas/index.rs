use axum::{response::Response, Extension};
use serde::Serialize;

use crate::{
    error::map_eyre_error,
    forecast_areas::{list_forecast_areas, ForecastAreaId},
    templates::TemplatesWithContext,
};

#[derive(Serialize)]
pub struct Context {
    forecast_area_ids: Vec<ForecastAreaId>,
}

pub async fn handler(
    Extension(templates): Extension<TemplatesWithContext>,
    Extension(database): Extension<crate::database::Database>,
) -> axum::response::Result<Response> {
    let context = Context {
        forecast_area_ids: list_forecast_areas(&database)
            .await
            .map_err(map_eyre_error)?,
    };
    templates
        .render("admin/forecast_areas/index.html", &context)
        .map_err(map_eyre_error)
        .map_err(Into::into)
}
