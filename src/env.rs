use std::{collections::HashMap, path::Path, env::VarError};

use color_eyre::Help;
use eyre::Context;
use serde::Deserialize;

use crate::options::Options;

const DOTENV_PATH: &str = ".env.toml";

pub fn initialize() -> eyre::Result<()> {
    let path = Path::new(DOTENV_PATH);
    if !path.exists() {
        return Ok(());
    }
    println!("INFO: loading environment variables from {path:?}");
    let dotenv =
        parse_dotenv(path).wrap_err_with(|| format!("Error loading dotenv file: {path:?}"))?;
    for (key, value) in dotenv {
        match std::env::var(&key) {
            Err(VarError::NotPresent) => {
                std::env::set_var(key, value)
            },
            _ => {
                tracing::info!("Environment variable {key} already set.")
            }
        }
    }
    Ok(())
}

fn parse_dotenv(path: &Path) -> eyre::Result<HashMap<String, String>> {
    let env_str = std::fs::read_to_string(path).wrap_err("Error loading dotenv file")?;
    let env: toml::Value = toml::from_str(&env_str).wrap_err("Error parsing dotenv file")?;
    let table: toml::value::Table = match env {
        toml::Value::Table(table) => table,
        unexpected => {
            return Err(eyre::eyre!("Unexpected dotenv format {unexpected:?}"))
                .suggestion("Should be a struct or map")
        }
    };
    table
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                toml::Value::String(value) => value,
                _ => match key.as_str() {
                    "OPTIONS" => toml::to_string(&Options::deserialize(value)?)?,
                    _ => toml::to_string(&value)?,
                },
            };

            Ok((key, value))
        })
        .collect()
}
