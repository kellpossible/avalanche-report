use std::str::FromStr;

use axum::{
    http::{header::CONTENT_TYPE, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};

use crate::database::Database;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/gudauri/area.geojson", get(gudauri_area_handler))
}

#[derive(sqlx::Type, Serialize, Deserialize, Debug)]
#[serde(transparent)]
#[sqlx(transparent)]
pub struct ForecastAreaId(String);

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

pub async fn gudauri_area_handler() -> impl IntoResponse {
    Response::builder()
        .header(
            CONTENT_TYPE,
            "application/geo+json".parse::<HeaderValue>().unwrap(),
        )
        .body(gudauri_area())
        .expect("Unable to build response")
}

pub fn gudauri_area() -> String {
    include_str!("./gudauri.geojson").to_string()
}
