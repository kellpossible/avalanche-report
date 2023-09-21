use std::{net::SocketAddr, num::NonZeroU32, path::PathBuf};

use cronchik::CronSchedule;
use eyre::Context;
use nonzero_ext::nonzero;
use serde::{ser::Error, Deserialize, Serialize};
use url::Url;

/// Global options for the application.
#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// Directory where application data is stored (including logs).
    ///
    /// Default is `data`.
    pub data_dir: PathBuf,
    /// Base url used for http server.
    ///
    /// Default is `http://{listen_address}/`.
    /// Can be specified by setting the environment variable `BASE_URL`.
    base_url: Option<url::Url>,
    /// Address by the http server for listening.
    ///
    /// Default is `127.0.0.1:3000`.
    pub listen_address: SocketAddr,
    /// The default selected langauge for the page (used when the user has not yet set a language
    /// or when their browser does not provide an Accept-Language header).
    pub default_language: unic_langid::LanguageIdentifier,
    /// Configuration for the map component.
    pub map: Map,
    /// Backup options.
    pub backup: Option<Backup>,
    /// Analytics options.
    pub analytics: Analytics,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Backup {
    #[serde(with = "serde_cron", default = "default_backup_schedule")]
    pub schedule: CronSchedule,
    pub s3_endpoint: Url,
    pub s3_bucket_name: String,
    pub s3_bucket_region: String,
    #[serde(default = "default_aws_access_key_id")]
    pub aws_access_key_id: String,
}

fn default_backup_schedule() -> CronSchedule {
    CronSchedule::parse_str("0 0 * * *").expect("Invalid cron schedule")
}

fn default_aws_access_key_id() -> String {
    std::env::var("AWS_ACCESS_KEY_ID").unwrap_or_default()
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MapTilerSource {
    api_key: Option<String>,
}

#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub enum MapSource {
    MapTiler(MapTilerSource),
    #[default]
    OpenTopoMap,
    Ersi,
}

#[derive(Default, Debug, Deserialize, Serialize, Clone)]
pub struct Map {
    source: MapSource,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            base_url: None,
            listen_address: default_listen_address(),
            default_language: default_default_language(),
            map: Map::default(),
            backup: None,
            analytics: Analytics::default(),
        }
    }
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
    /// Initialize the options using the `OPTIONS` environment variable. If `OPTIONS` contains a
    /// file path, it will load the options from that path, if `OPTIONS` contains a RON file
    /// definition then it will load the options from the string contained in the variable.
    pub async fn initialize() -> eyre::Result<Options> {
        let result = match std::env::var("OPTIONS") {
            Ok(options) => match toml::from_str(&options) {
                Ok(options) => {
                    println!("INFO: Options loaded from `OPTIONS` environment variable");
                    Ok(options)
                }
                Err(error) => {
                    let path = PathBuf::from(&options);
                    if path.is_file() {
                        let options_str = tokio::fs::read_to_string(&path).await?;
                        let options: Options =
                            toml::from_str(&options_str).wrap_err_with(|| {
                                format!("Error deserializing options file: {:?}", path)
                            })?;
                        println!("INFO: Options loaded from file specified in `OPTIONS` environment variable: {:?}", path);
                        Ok(options)
                    } else {
                        Err(error).wrap_err_with(|| {
                            format!(
                                "Error deserializing options from `OPTIONS` environment variable \
                            string, or you have specified a file path which does not exist: {:?}",
                                options
                            )
                        })
                    }
                }
            },
            Err(std::env::VarError::NotPresent) => {
                println!("INFO: No OPTIONS environment variable found, using default options.");
                Ok(Options::default())
            }
            Err(error) => Err(error).wrap_err("Error reading `OPTIONS` environment variable"),
        };
        if let Ok(options) = &result {
            println!("INFO: Options:\n\x1b[34m{options}\x1b[0m")
        }
        result
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
