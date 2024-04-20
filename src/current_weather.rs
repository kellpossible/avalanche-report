//! Module for fetching the current weather data from weather stations.

use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    response::{ErrorResponse, IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use eyre::{bail, Context, ContextCompat};
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

#[derive(Serialize, Deserialize, Debug)]
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
        // Type override to workaround https://github.com/launchbadge/sqlx/issues/1979
        Ok(sqlx::query_as!(
            CurrentWeatherCache,
            r#"SELECT weather_station_id, data as "data!: sqlx::types::Json<Vec<WeatherDataItem>>" FROM current_weather_cache WHERE weather_station_id = ?"#,
            id,
        )
        .fetch_all(&self.database)
        .await
        .wrap_err("Error fetching current weather cache item")?
        .into_iter()
        .flat_map(|row| row.data.0)
        .collect())
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

pub struct CurrentWeatherCache {
    pub weather_station_id: WeatherStationId,
    pub data: sqlx::types::Json<Vec<WeatherDataItem>>,
}

impl CurrentWeatherCacheService {
    pub fn try_new(config: CurrentWeatherCacheServiceConfig) -> eyre::Result<Self> {
        if config
            .each_station_interval
            .saturating_mul(config.weather_stations.len().try_into()?)
            > config.interval
        {
            bail!(
                "Cannot create CurrentWeatherCacheService, invalid config, not enough time to complete requests {} per each {} weather stations within {}", 
                humantime::format_duration(config.each_station_interval),
                config.weather_stations.len(),
                humantime::format_duration(config.interval)
            )
        }
        Ok(Self { config })
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
            data: sqlx::types::Json(weather_data),
        };

        sqlx::query!(
            "INSERT INTO current_weather_cache VALUES($1, $2) ON CONFLICT(weather_station_id) DO UPDATE SET data=excluded.data",
            current_weather.weather_station_id,
            current_weather.data,
        ).execute(&self.config.database).await?;
        Ok(())
    }

    async fn fetch_and_cache_current_weather(&self) -> eyre::Result<()> {
        loop {
            let before_requests_time = tokio::time::Instant::now();
            for (id, station) in self.config.weather_stations {
                if let Err(error) = self.fetch_and_update_station(id, station).await {
                    tracing::error!(
                        "Error fetching and updating weather data for station {id}: {error:?}"
                    );
                }
                tokio::time::sleep(self.config.each_station_interval).await;
            }
            let after_requests_time = tokio::time::Instant::now();
            let requests_duration = after_requests_time - before_requests_time;
            tokio::time::sleep(std::time::Duration::max(
                self.config.interval.saturating_sub(requests_duration),
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

#[derive(Serialize, Debug)]
pub struct CurrentWeatherContext {
    pub weather_stations: HashMap<WeatherStationId, Vec<WeatherDataItem>>,
    pub wind_unit: WindUnit,
}

impl CurrentWeatherContext {
    pub async fn from_service(
        service: &CurrentWeatherService,
        wind_unit: WindUnit,
    ) -> eyre::Result<Self> {
        let mut weather_stations = HashMap::new();
        for id in service.available_weather_stations() {
            let data = service.current_weather(&id).await?;
            weather_stations.insert(id, data);
        }
        Ok(Self {
            weather_stations,
            wind_unit,
        })
    }
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
) -> axum::response::Result<Response> {
    tracing::debug!("Getting current weather");
    let context = CurrentWeatherContext::from_service(
        &service,
        query
            .wind_unit
            .or(preferences.wind_unit)
            .unwrap_or_default(),
    )
    .await
    .map_err(map_eyre_error)?;
    Ok(
        render(&templates.environment, "current_weather.html", &context)
            .wrap_err("Error rendering current weather template")
            .map_err(map_eyre_error)
            .into_response(),
    )
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
        .map_err(Into::into)
        .map(Json)
}
