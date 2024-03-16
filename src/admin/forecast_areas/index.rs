use axum::{response::Response, Extension};

use crate::{database::DatabaseInstance, error::map_eyre_error, templates::TemplatesWithContext};

pub async fn handler(
    Extension(templates): Extension<TemplatesWithContext>,
    Extension(database): Extension<DatabaseInstance>,
) -> axum::response::Result<Response> {
    tracing::info!("Forecast areas!!");
    templates
        .render("admin/forecast_areas/index.html", &())
        .map_err(map_eyre_error)
}
