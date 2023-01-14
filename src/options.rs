use axum_reporting::DelayedLogs;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

use eyre::Context;
use ron::ser::PrettyConfig;
use serde::{ser::Error, Deserialize, Serialize};
use tracing::Level;

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
}

impl Default for Options {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            base_url: default_base_url(),
            listen_address: default_listen_address(),
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

impl std::fmt::Display for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let options_str = ron::ser::to_string_pretty(self, PrettyConfig::default())
            .map_err(|error| std::fmt::Error::custom(error))?;
        f.write_str("Options")?;
        f.write_str(&options_str)
    }
}

/// Result of [`Options::initialize()`].
pub struct OptionsInit {
    /// Options that were initialized.
    pub result: eyre::Result<Options>,
    /// Messages that are destined to logged after tracing has been
    /// initialized.
    pub logs: DelayedLogs,
}

impl Options {
    /// Initialize the options using the `OPTIONS` environment variable, otherwise load from file
    /// `options.ron` by default. If `OPTIONS` contains a file path, it will load the options from
    /// that path, if `OPTIONS` contains a RON file definition then it will load the options from
    /// the string contained in the variable.
    pub async fn initialize() -> OptionsInit {
        let mut logs = DelayedLogs::default();
        let result = initialize_impl(&mut logs).await;

        OptionsInit { result, logs }
    }
}

async fn initialize_impl(logs: &mut DelayedLogs) -> eyre::Result<Options> {
    let result = match std::env::var("OPTIONS") {
        Ok(options) => match ron::from_str(&options) {
            Ok(options) => {
                logs.push(
                    Level::INFO,
                    "Options loaded from `OPTIONS` environment variable",
                );
                Ok(options)
            }
            Err(error) => {
                let path = PathBuf::from(&options);
                if path.is_file() {
                    let options_str = tokio::fs::read_to_string(&path).await?;
                    let options: Options = ron::from_str(&options_str).wrap_err_with(|| {
                        format!("Error deserializing options file: {:?}", path)
                    })?;
                    logs.push(Level::INFO, format!("Options loaded from file specified in `OPTIONS` environment variable: {:?}", path));
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
            let path = Path::new("options.ron");
            if !path.is_file() {
                logs.push(Level::INFO, "No options found, using defaults");
                return Ok(Options::default());
            }
            let options_str = tokio::fs::read_to_string(&path).await?;
            let options = ron::from_str(&options_str)
                .wrap_err_with(|| format!("Error deserializing options file: {:?}", path))?;

            logs.push(
                Level::INFO,
                format!("Options loaded from default file: {:?}", path),
            );

            Ok(options)
        }
        Err(error) => return Err(error).wrap_err("Error reading `OPTIONS` environment variable"),
    };
    if let Ok(options) = &result {
        logs.push(Level::INFO, format!("{}", options));
    }
    result
}
