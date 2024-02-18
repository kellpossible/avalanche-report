//! Module for fetching the current weather data from weather stations.

use eyre::ContextCompat;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

use crate::options::AmbientWeatherSource;

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

pub struct WeatherDataItem {
    pub time: time::OffsetDateTime,
    pub temperature_celcius: Option<f64>,
    pub wind_direction_degrees: Option<f64>,
    pub wind_speed_ms: Option<f64>,
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
        })
    }
}

pub struct AmbientWeatherRepository {
    client: reqwest::Client,
}

#[derive(Serialize)]
pub struct DeviceDataQuery<'a> {
    mac_address: &'a str,
    api_key: &'a str,
    application_key: &'a str,
    #[serde(with = "time::serde::iso8601::option")]
    end_date: Option<time::OffsetDateTime>,
    limit: Option<i64>,
}

impl AmbientWeatherRepository {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub async fn query_device_data(
        &self,
        source: &AmbientWeatherSource,
    ) -> eyre::Result<Vec<WeatherDataItem>> {
        let now = time::OffsetDateTime::now_utc();
        let query = DeviceDataQuery {
            mac_address: &source.device_mac_address,
            api_key: source.api_key.expose_secret(),
            application_key: source.application_key.expose_secret(),
            end_date: Some(now),
            limit: None,
        };
        self
            .client
            .get("https://ambientweather.net")
            .query(&query)
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<QueryDeviceDataResponseItem>>()
            .await?
            .into_iter()
            .map(WeatherDataItem::try_from)
            .collect()
    }
}
