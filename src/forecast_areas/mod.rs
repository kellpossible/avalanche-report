use axum::{
    extract,
    http::{header::CONTENT_TYPE, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use eyre::ContextCompat;
use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{database::Database, error::map_eyre_error};

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/:id/area.geojson", get(handler))
}

#[derive(sqlx::Type, Serialize, Deserialize, Debug)]
#[serde(transparent)]
#[sqlx(transparent)]
pub struct ForecastAreaId(String);

impl std::fmt::Display for ForecastAreaId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for ForecastAreaId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Debug)]
pub struct ForecastArea {
    pub id: ForecastAreaId,
    pub geojson: serde_json::Value,
}

pub async fn list_forecast_areas(database: &Database) -> eyre::Result<Vec<ForecastAreaId>> {
    Ok(sqlx::query!("SELECT id FROM forecast_areas")
        .fetch_all(database)
        .await?
        .into_iter()
        .map(|record| ForecastAreaId(record.id))
        .collect::<Vec<_>>())
}

pub async fn upsert_forecast_area(
    database: &Database,
    forecast_area: ForecastArea,
) -> eyre::Result<()> {
    sqlx::query!(
        "INSERT INTO forecast_areas VALUES($1, $2) ON CONFLICT(id) DO UPDATE SET geojson=$2",
        forecast_area.id,
        forecast_area.geojson,
    )
    .execute(database)
    .await?;
    Ok(())
}

pub async fn get_forecast_area(
    database: &Database,
    id: &ForecastAreaId,
) -> eyre::Result<Option<ForecastArea>> {
    Ok(sqlx::query_as!(
        ForecastArea,
        r#"SELECT id, geojson as "geojson!: _" FROM forecast_areas WHERE id=$1"#,
        id
    )
    .fetch_optional(database)
    .await?)
}

#[derive(Deserialize)]
pub struct PathParams {
    id: ForecastAreaId,
}

pub async fn handler(
    extract::Path(path): extract::Path<PathParams>,
    Extension(database): Extension<Database>,
) -> axum::response::Result<Response> {
    let forecast_area = get_forecast_area(&database, &path.id)
        .await
        .map_err(map_eyre_error)?
        .wrap_err_with(|| format!("No forecast area found for id {}", path.id))
        .map_err(|not_found| {
            let mut response = map_eyre_error(not_found);
            *response.status_mut() = StatusCode::NOT_FOUND;
            response
        })?;
    let mut response = Json(forecast_area.geojson).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        "application/geo+json".parse::<HeaderValue>().unwrap(),
    );
    Ok(response)
}
