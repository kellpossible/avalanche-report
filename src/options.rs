use std::{collections::HashMap, net::SocketAddr, num::NonZeroU32, path::PathBuf};

use crate::serde::hide_secret;
use cronchik::CronSchedule;
use eyre::ContextCompat;
use nonzero_ext::nonzero;
use secrecy::SecretString;
use serde::{ser::Error, Deserialize, Serialize};
use serde_with::{serde_as, EnumMap};
use toml_env::AutoMapEnvArgs;
use url::Url;

/// Global options for the application.
#[derive(Debug, Serialize, Deserialize)]
pub struct Options {
    /// Directory where application data is stored (including logs).
    ///
    /// Default is `data`.
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// Base url used for http server.
    /// Can be specified by setting the environment variable `BASE_URL`.
    ///
    /// Default is `http://{listen_address}/`.
    #[serde(default)]
    base_url: Option<url::Url>,
    /// Address by the http server for listening.
    ///
    /// Default is `127.0.0.1:3000`.
    #[serde(default = "default_listen_address")]
    pub listen_address: SocketAddr,
    /// The default selected langauge for the page (used when the user has not yet set a language
    /// or when their browser does not provide an Accept-Language header).
    #[serde(default = "default_default_language_order")]
    pub default_language_order: Vec<unic_langid::LanguageIdentifier>,
    /// See [`Map`].
    #[serde(default)]
    pub map: Map,
    /// See [`Backup`].
    #[serde(default)]
    pub backup: Option<Backup>,
    /// See [`Analytics`].
    #[serde(default)]
    pub analytics: Analytics,
    /// See [`GoogleDrive`].
    pub google_drive: GoogleDrive,
    #[serde(serialize_with = "hide_secret::serialize")]
    /// (REQUIRED) Hash of the `admin` user password, used to access `/admin/*` routes.
    pub admin_password_hash: SecretString,
    /// See [`WeatherMap`].
    #[serde(default)]
    pub weather_maps: WeatherMaps,
    /// See [`WeatherStation`].
    #[serde(default)]
    pub weather_stations: HashMap<WeatherStationId, WeatherStation>,
    /// See [`I18n`].
    #[serde(default)]
    pub i18n: I18n,
    /// See [`Templates`].
    #[serde(default)]
    pub templates: Templates,
}

/// Configuration for the HTML templates.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Templates {
    /// The path to the directory containing overrides for templates.
    pub directory: Option<PathBuf>,
}

/// Configuration for application localization.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct I18n {
    /// The path to the directory containing overrides for localization resources.
    pub directory: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone, sqlx::Type)]
#[serde(transparent)]
#[sqlx(transparent)]
pub struct WeatherStationId(String);

impl From<String> for WeatherStationId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for WeatherStationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Configuration for using Google Drive.
#[derive(Debug, Serialize, Deserialize)]
pub struct GoogleDrive {
    /// The identifier for the folder in Google Drive where the published forecasts are stored.
    pub published_folder_id: String,
    /// Google Drive API key, used to access forecast spreadsheets.
    #[serde(serialize_with = "hide_secret::serialize")]
    pub api_key: SecretString,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct WeatherMaps(#[serde_as(as = "EnumMap")] Vec<WeatherMap>);

impl From<WeatherMaps> for Vec<WeatherMap> {
    fn from(value: WeatherMaps) -> Self {
        value.0
    }
}

/// Include a current weather map on the forecast page.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum WeatherMap {
    /// See [WindyWeather].
    #[serde(alias = "windy")]
    Windy(WindyWeather),
    #[serde(alias = "meteoblue")]
    Meteoblue(MeteoblueWeather),
}

/// Weather map from <https://meteoblue.com>
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MeteoblueWeather {
    pub location_id: String,
}

/// Weather map from <https://windy.com>
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WindyWeather {
    pub latitude: f64,
    pub longitude: f64,
}

/// `avalanche-report` has a built-in backup facility which can save the database and push it to an
/// amazon s3 compatible storage API.
#[derive(Debug, Serialize, Deserialize)]
pub struct Backup {
    /// Schedule for when the backup is performed.
    ///
    /// Default is `0 0 * * *`.
    #[serde(with = "serde_cron", default = "default_backup_schedule")]
    pub schedule: CronSchedule,
    #[serde(serialize_with = "hide_secret::serialize")]
    pub aws_secret_access_key: SecretString,
    pub s3_endpoint: Url,
    pub s3_bucket_name: String,
    pub s3_bucket_region: String,
    pub aws_access_key_id: String,
}

fn default_backup_schedule() -> CronSchedule {
    CronSchedule::parse_str("0 0 * * *").expect("Invalid cron schedule")
}

mod serde_cron {
    use cronchik::CronSchedule;

    pub fn serialize<S>(value: &CronSchedule, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<CronSchedule, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = CronSchedule;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("Expecting a valid cron job string. e.g. \"5 * * * *\"")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                CronSchedule::parse_str(v).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

/// `avalanche-report` has a built-in server-side analytics collection mechanism.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Analytics {
    /// Schedule for when analytics data compaction is performed.
    ///
    /// Default is `0 1 * * *`.
    #[serde(with = "serde_cron")]
    pub compaction_schedule: CronSchedule,
    /// Number of analytics event batches that will be submited to the database per hour.
    ///
    /// Default is 60 (one time per minute).
    pub event_batch_rate: NonZeroU32,
}

impl Default for Analytics {
    fn default() -> Self {
        Self {
            compaction_schedule: CronSchedule::parse_str("0 1 * * *")
                .expect("Invalid cron schedule"),
            event_batch_rate: default_analytics_batch_rate(),
        }
    }
}

impl Options {
    pub fn base_url(&self) -> url::Url {
        self.base_url.clone().unwrap_or_else(|| {
            format!(
                "http://{0}:{1}/",
                self.listen_address.ip(),
                self.listen_address.port()
            )
            .parse()
            .expect("Unable to parse base url")
        })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub enum MapTilerStyle {
    #[serde(alias = "topo", alias = "topo-v2")]
    Topo,
    #[serde(alias = "winter", alias = "winter-v2")]
    #[default]
    Winter,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MapTilerSource {
    api_key: Option<String>,
    /// Sets the map style.
    #[serde(default)]
    style: MapTilerStyle,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TracestrackSource {
    api_key: Option<String>,
}

#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub enum MapSource {
    #[serde(alias = "map_tiler")]
    MapTiler(MapTilerSource),
    #[default]
    #[serde(alias = "open_topo_map")]
    OpenTopoMap,
    #[serde(alias = "ersi")]
    Ersi,
    #[serde(alias = "tracestrack")]
    Tracestrack(TracestrackSource),
}

/// Configuration for the map component.
#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub struct Map {
    /// The source for the basemap of the map component.
    /// Default is [`MapSource::OpenTopoMap`].
    pub source: MapSource,
}

fn default_data_dir() -> PathBuf {
    "data".into()
}

fn default_listen_address() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 3000))
}

fn default_analytics_batch_rate() -> NonZeroU32 {
    nonzero!(60u32)
}

fn default_default_language_order() -> Vec<unic_langid::LanguageIdentifier> {
    vec!["en-UK"
        .parse()
        .expect("Unable to parse language identifier")]
}

impl std::fmt::Display for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let options_str = toml::ser::to_string_pretty(self).map_err(std::fmt::Error::custom)?;
        f.write_str(&options_str)
    }
}

impl Options {
    /// Initialize options using the [`toml_env`] library.
    pub async fn initialize() -> eyre::Result<Options> {
        toml_env::initialize(toml_env::Args {
            config_variable_name: "AVALANCHE_REPORT",
            logging: toml_env::Logging::StdOut,
            auto_map_env: Some(AutoMapEnvArgs {
                prefix: Some("AVALANCHE_REPORT"),
                ..AutoMapEnvArgs::default()
            }),
            ..toml_env::Args::default()
        })?
        .wrap_err("No configuration specified")
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum WeatherStationSource {
    /// See [`AmbientWeatherSource`].
    #[serde(alias = "ambient_weather")]
    AmbientWeather(AmbientWeatherSource),
}

/// Weather source from <https://ambientweather.net>
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AmbientWeatherSource {
    pub device_mac_address: String,
    #[serde(serialize_with = "hide_secret::serialize")]
    pub api_key: SecretString,
    #[serde(serialize_with = "hide_secret::serialize")]
    pub application_key: SecretString,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WeatherStation {
    /// Where the weather station data is pulled from.
    pub source: WeatherStationSource,
}

#[cfg(test)]
mod test {
    use insta::assert_json_snapshot;
    use serde_json::json;

    use super::Backup;

    #[test]
    fn parse_backup() {
        let value = json!({ "schedule": "0 1,2,3,4,5 * * *"});
        let backup: Backup = serde_json::from_value(value.clone()).unwrap();
        assert_json_snapshot!(backup, @r###"
        {
          "schedule": "0 1-5 * * *"
        }
        "###);

        let value = json!({ "schedule": "1/10 * * * *"});
        let backup: Backup = serde_json::from_value(value.clone()).unwrap();
        assert_json_snapshot!(backup, @r###"
        {
          "schedule": "1,11,21,31,41,51 * * * *"
        }
        "###);
    }
}
