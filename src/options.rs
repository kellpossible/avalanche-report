use std::{collections::HashMap, net::SocketAddr, num::NonZeroU32, path::PathBuf};

use crate::serde::hide_secret;
use cronchik::CronSchedule;
use eyre::ContextCompat;
use nonzero_ext::nonzero;
use secrecy::SecretString;
use serde::{ser::Error, Deserialize, Serialize};
use toml_env::TomlKeyPath;
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
    ///
    /// Default is `http://{listen_address}/`.
    /// Can be specified by setting the environment variable `BASE_URL`.
    #[serde(default)]
    base_url: Option<url::Url>,
    /// Address by the http server for listening.
    ///
    /// Default is `127.0.0.1:3000`.
    #[serde(default = "default_listen_address")]
    pub listen_address: SocketAddr,
    /// The default selected langauge for the page (used when the user has not yet set a language
    /// or when their browser does not provide an Accept-Language header).
    #[serde(default = "default_default_language")]
    pub default_language: unic_langid::LanguageIdentifier,
    /// Configuration for the map component.
    #[serde(default)]
    pub map: Map,
    /// Backup options.
    #[serde(default)]
    pub backup: Option<Backup>,
    /// Analytics options.
    #[serde(default)]
    pub analytics: Analytics,
    /// Google Drive API key, used to access forecast spreadsheets.
    #[serde(serialize_with = "hide_secret::serialize")]
    pub google_drive_api_key: SecretString,
    #[serde(serialize_with = "hide_secret::serialize")]
    /// Hash of the `admin` user password, used to access `/admin/*` rousted.
    pub admin_password_hash: SecretString,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Backup {
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Analytics {
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
    #[serde(rename = "topo-v2")]
    Topo,
    #[serde(rename = "winter-v2")]
    #[default]
    Winter
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MapTilerSource {
    api_key: Option<String>,
    #[serde(default)]
    style: MapTilerStyle,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TracestrackSource {
    api_key: Option<String>,
}

#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub enum MapSource {
    MapTiler(MapTilerSource),
    #[default]
    OpenTopoMap,
    Ersi,
    Tracestrack(TracestrackSource)
}

#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub struct Map {
    source: MapSource,
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

fn default_default_language() -> unic_langid::LanguageIdentifier {
    "en-UK"
        .parse()
        .expect("Unable to parse language identifier")
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
            config_variable_name: "OPTIONS",
            logging: toml_env::Logging::StdOut,
            map_env: [
                ("ADMIN_PASSWORD_HASH", "admin_password_hash"),
                ("GOOGLE_DRIVE_API_KEY", "google_drive_api_key"),
                ("AWS_ACCESS_KEY_ID", "backup.aws_access_key_id"),
                ("AWS_SECRET_ACCESS_KEY", "backup.aws_secret_access_key"),
            ]
            .into_iter()
            .map(|(key, value)| Ok((key, value.parse::<TomlKeyPath>()?)))
            .collect::<eyre::Result<HashMap<_, _>>>()?,
            ..toml_env::Args::default()
        })?
        .wrap_err("No configuration specified")
    }
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
