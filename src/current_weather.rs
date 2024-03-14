//! Module for fetching the current weather data from weather stations.

use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    response::{ErrorResponse, IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use eyre::{Context, ContextCompat};
use rusqlite::{types::Type, Row};
use sea_query::{Expr, OnConflict, SimpleExpr, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use tracing::Instrument;

use crate::{
    database::Database,
    error::map_eyre_error,
    options::{AmbientWeatherSource, WeatherStation, WeatherStationId},
    state::AppState,
    templates::{render, TemplatesWithContext},
    user_preferences::{UserPreferences, WindUnit},
};

#[derive(Clone, Debug, Deserialize)]
pub struct QueryDeviceDataResponseItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baromabsin: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baromrelin: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dailyrainin: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "time::serde::rfc3339::option"
    )]
    pub date: Option<time::OffsetDateTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dateutc: Option<f64>,
    #[serde(rename = "dewPoint", default, skip_serializing_if = "Option::is_none")]
    pub dew_point: Option<f64>,
    #[serde(rename = "feelsLike", default, skip_serializing_if = "Option::is_none")]
    pub feels_like: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hourlyrainin: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub humidity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub humidityin: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maxdailygust: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monthlyrainin: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempf: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempinf: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winddir: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winddir_avg10m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winddir_avg2m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windgustdir: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windgustmph: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windspdmph_avg10m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windspdmph_avg2m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windspeedmph: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yearlyrainin: Option<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct WeatherDataItem {
    #[serde(with = "time::serde::rfc3339")]
    pub time: time::OffsetDateTime,
    pub temperature_celcius: Option<f64>,
    pub wind_direction_degrees: Option<f64>,
    pub wind_speed_ms: Option<f64>,
    pub humidity_percent: Option<f64>,
}

fn farenheit_to_celcius(temperature: f64) -> f64 {
    (temperature - 32.0) * 5.0 / 9.0
}

fn mph_to_ms(speed: f64) -> f64 {
    speed * 0.44704
}

impl TryFrom<QueryDeviceDataResponseItem> for WeatherDataItem {
    type Error = eyre::Error;

    fn try_from(value: QueryDeviceDataResponseItem) -> Result<Self, Self::Error> {
        Ok(Self {
            time: value.date.context("date field missing")?,
            temperature_celcius: value.tempf.map(farenheit_to_celcius),
            wind_direction_degrees: value.winddir,
            wind_speed_ms: value.windspeedmph.map(mph_to_ms),
            humidity_percent: value.humidity,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDataQuery<'a> {
    api_key: &'a str,
    application_key: &'a str,
    #[serde(with = "time::serde::iso8601::option")]
    end_date: Option<time::OffsetDateTime>,
    limit: Option<i64>,
}

pub struct CurrentWeatherService {
    database: Database,
    weather_stations: HashMap<WeatherStationId, WeatherStation>,
}

impl CurrentWeatherService {
    pub fn new(
        database: Database,
        weather_stations: HashMap<WeatherStationId, WeatherStation>,
    ) -> Self {
        Self {
            database,
            weather_stations,
        }
    }
}

impl CurrentWeatherService {
    pub fn available_weather_stations(&self) -> Vec<WeatherStationId> {
        self.weather_stations.keys().cloned().collect()
    }

    pub async fn current_weather(
        &self,
        id: &WeatherStationId,
    ) -> eyre::Result<Vec<WeatherDataItem>> {
        let database = self.database.get().await?;
        let id = id.clone();
        database
            .interact(move |conn| {
                let id_clone = id.clone();
                let mut query = sea_query::Query::select();
                query
                    .columns(CurrentWeatherCache::COLUMNS)
                    .from(CurrentWeatherCache::TABLE)
                    .and_where(Expr::col(CurrentWeatherCacheIden::WeatherStationId).eq(id));
                let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
                let mut statement = conn.prepare_cached(&sql)?;
                let data: Vec<WeatherDataItem> = Option::transpose(
                    statement
                        .query_map(&*values.as_params(), |row| {
                            CurrentWeatherCache::try_from(row)
                        })
                        .wrap_err("Error performing query to obtain `ForecastFiles`")?
                        .next(),
                )?
                .map(|cache_item| cache_item.data)
                .unwrap_or_else(|| {
                    tracing::warn!("No cached weather data can be found for {id_clone}");
                    Default::default()
                });

                eyre::Ok(data)
            })
            .await?
    }
}

#[derive(Deserialize)]
pub struct PathParams {
    weather_station_id: WeatherStationId,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(handler))
        .route(
            "/weather-station/:weather_station_id",
            get(weather_station_handler),
        )
        .route(
            "/available-weather-stations",
            get(available_weather_stations_handler),
        )
}

pub struct CurrentWeatherCacheServiceConfig {
    pub interval: std::time::Duration,
    pub each_station_interval: std::time::Duration,
    pub weather_stations: &'static HashMap<WeatherStationId, WeatherStation>,
    pub client: reqwest::Client,
    pub database: Database,
}

/// Service for fetching and caching current weather data to avoid API limits and improve
/// durability.
pub struct CurrentWeatherCacheService {
    config: CurrentWeatherCacheServiceConfig,
}

#[sea_query::enum_def]
pub struct CurrentWeatherCache {
    pub weather_station_id: WeatherStationId,
    pub data: Vec<WeatherDataItem>,
}

impl AsRef<str> for CurrentWeatherCacheIden {
    fn as_ref(&self) -> &str {
        match self {
            Self::Table => "current_weather_cache",
            Self::WeatherStationId => "weather_station_id",
            Self::Data => "data",
        }
    }
}

impl CurrentWeatherCache {
    pub const COLUMNS: [CurrentWeatherCacheIden; 2] = [
        CurrentWeatherCacheIden::WeatherStationId,
        CurrentWeatherCacheIden::Data,
    ];
    pub const TABLE: CurrentWeatherCacheIden = CurrentWeatherCacheIden::Table;

    pub fn values(self) -> eyre::Result<[SimpleExpr; 2]> {
        Ok([
            self.weather_station_id.into(),
            serde_json::to_string(&self.data)?.into(),
        ])
    }
}

impl TryFrom<&Row<'_>> for CurrentWeatherCache {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        let weather_station_id = row.get(CurrentWeatherCacheIden::WeatherStationId.as_ref())?;
        let data: serde_json::Value = row.get(CurrentWeatherCacheIden::Data.as_ref())?;
        let data = serde_json::from_value(data)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(2, Type::Text, Box::new(e)))?;

        Ok(CurrentWeatherCache {
            weather_station_id,
            data,
        })
    }
}

impl CurrentWeatherCacheService {
    pub fn new(config: CurrentWeatherCacheServiceConfig) -> Self {
        Self { config }
    }

    async fn fetch_and_update_station(
        &self,
        id: &WeatherStationId,
        station: &WeatherStation,
    ) -> eyre::Result<()> {
        let weather_data = match &station.source {
            crate::options::WeatherStationSource::AmbientWeather(source) => self
                .ambient_weather_query_device_data(source)
                .await
                .wrap_err("Error querying ambient weather device data")?,
        };
        let current_weather = CurrentWeatherCache {
            weather_station_id: id.clone(),
            data: weather_data,
        };
        let database = self.config.database.get().await?;
        database
            .interact(move |conn| {
                let mut query = sea_query::Query::insert();
                let values = current_weather.values()?;
                query
                    .into_table(CurrentWeatherCache::TABLE)
                    .columns(CurrentWeatherCache::COLUMNS)
                    .values(values.clone())?;
                query.on_conflict(
                    OnConflict::column(CurrentWeatherCacheIden::WeatherStationId)
                        .values(
                            CurrentWeatherCache::COLUMNS[1..]
                                .into_iter()
                                .zip(values[1..].into_iter())
                                .map(|(c, v)| (*c, v.clone())),
                        )
                        .to_owned(),
                );
                let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
                conn.execute(&sql, &*values.as_params())?;
                eyre::Ok(())
            })
            .await??;
        Ok(())
    }

    async fn fetch_and_cache_current_weather(&self) -> eyre::Result<()> {
        loop {
            let before_requests_time = std::time::Instant::now();
            for (id, station) in self.config.weather_stations {
                if let Err(error) = self.fetch_and_update_station(id, station).await {
                    tracing::error!(
                        "Error fetching and updating weather data for station {id}: {error:?}"
                    );
                }
                tokio::time::sleep(self.config.each_station_interval).await;
            }
            let after_requests_time = std::time::Instant::now();
            let requests_duration = after_requests_time - before_requests_time;
            tokio::time::sleep(std::time::Duration::max(
                self.config.interval - requests_duration,
                self.config.each_station_interval,
            ))
            .await;
        }
    }

    async fn ambient_weather_query_device_data(
        &self,
        source: &AmbientWeatherSource,
    ) -> eyre::Result<Vec<WeatherDataItem>> {
        let now = time::OffsetDateTime::now_utc();
        let query = DeviceDataQuery {
            api_key: source.api_key.expose_secret(),
            application_key: source.application_key.expose_secret(),
            end_date: Some(now),
            limit: None,
        };
        let mac_address = &source.device_mac_address;
        self.config
            .client
            .get(format!(
                "https://rt.ambientweather.net/v1/devices/{mac_address}"
            ))
            .query(&query)
            .send()
            .await?
            .error_for_status()
            .wrap_err("Status code of response is an error")?
            .json::<Vec<QueryDeviceDataResponseItem>>()
            .await
            .wrap_err("Error deserializing response body")?
            .into_iter()
            .map(WeatherDataItem::try_from)
            .collect()
    }

    pub fn spawn(self) {
        tokio::spawn(
            async move {
                tracing::info!("Spawned current weather cache service");
                loop {
                    let before_requests_time = std::time::Instant::now();
                    if let Err(error) = self.fetch_and_cache_current_weather().await {
                        tracing::error!("Error fetching and caching current weather: {error:?}")
                    };
                    let after_requests_time = std::time::Instant::now();
                    let requests_duration = after_requests_time - before_requests_time;
                    tokio::time::sleep(std::time::Duration::max(
                        self.config.interval - requests_duration,
                        self.config.each_station_interval,
                    ))
                    .await;
                }
            }
            .instrument(tracing::error_span!("current_weather_cache")),
        );
    }
}

#[derive(Serialize)]
struct CurrentWeatherContext {
    weather_stations: HashMap<WeatherStationId, Vec<WeatherDataItem>>,
    wind_unit: WindUnit,
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct Query {
    wind_unit: Option<WindUnit>,
}

pub async fn handler(
    axum::extract::Query(query): axum::extract::Query<Query>,
    State(service): State<std::sync::Arc<CurrentWeatherService>>,
    Extension(preferences): Extension<UserPreferences>,
    Extension(templates): Extension<TemplatesWithContext>,
) -> Response {
    tracing::info!("Getting current weather");
    let mut weather_stations = HashMap::new();
    for id in service.available_weather_stations() {
        let data = service.current_weather(&id).await.unwrap();
        weather_stations.insert(id, data);
    }
    let context = CurrentWeatherContext {
        weather_stations,
        wind_unit: query
            .wind_unit
            .or(preferences.wind_unit)
            .unwrap_or_default(),
    };
    render(&templates.environment, "current_weather.html", &context)
        .wrap_err("Error rendering current weather template")
        .map_err(map_eyre_error)
        .into_response()
}

pub async fn available_weather_stations_handler(
    State(service): State<std::sync::Arc<CurrentWeatherService>>,
) -> Json<Vec<WeatherStationId>> {
    Json(service.available_weather_stations())
}

pub async fn weather_station_handler(
    Path(path): Path<PathParams>,
    State(service): State<std::sync::Arc<CurrentWeatherService>>,
) -> axum::response::Result<Json<Vec<WeatherDataItem>>, ErrorResponse> {
    service
        .current_weather(&path.weather_station_id)
        .await
        .map_err(map_eyre_error)
        .map(Json)
}
