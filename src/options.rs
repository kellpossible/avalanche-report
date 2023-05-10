use std::{net::SocketAddr, num::NonZeroU32, path::PathBuf};

use eyre::Context;
use nonzero_ext::nonzero;
use serde::{ser::Error, Deserialize, Serialize};

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
    /// Default is `http://localhost:3000/`.
    /// Can be specified by setting the environment variable `BASE_URL`.
    #[serde(default = "default_base_url")]
    pub base_url: url::Url,
    /// Address by the http server for listening.
    ///
    /// Default is `127.0.0.1:3000`.
    #[serde(default = "default_listen_address")]
    pub listen_address: SocketAddr,
    /// Number of analytics event batches that will be submited to the database per hour.
    ///
    /// Default is 60 (one time per minute).
    #[serde(default = "default_analytics_batch_rate")]
    pub analytics_batch_rate: NonZeroU32,
    /// The default selected langauge for the page (used when the user has not yet set a language
    /// or when their browser does not provide an Accept-Language header).
    #[serde(default = "default_default_language")]
    pub default_language: unic_langid::LanguageIdentifier,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            base_url: default_base_url(),
            listen_address: default_listen_address(),
            analytics_batch_rate: default_analytics_batch_rate(),
            default_language: default_default_language(),
        }
    }
}

fn default_data_dir() -> PathBuf {
    "data".into()
}

fn default_base_url() -> url::Url {
    "http://localhost:3000"
        .parse()
        .expect("Unable to parse url")
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
