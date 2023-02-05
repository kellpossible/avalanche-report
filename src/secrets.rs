use std::{env::VarError, path::Path};

use eyre::Context;
use secrecy::SecretString;

pub struct Secrets {
    pub google_drive_api_key: Option<SecretString>,
}

impl Secrets {
    pub async fn initialize(secrets_dir: &Path) -> eyre::Result<Self> {
        let google_drive_api_key = match std::env::var("GOOGLE_DRIVE_API_KEY") {
            Ok(google_drive_api_key) => {
                tracing::info!(
                    "Google Drive api key was read from GOOGLE_DRIVE_API_KEY environment variable"
                );
                Some(SecretString::new(google_drive_api_key))
            }
            Err(VarError::NotPresent) => {
                let api_key_path = secrets_dir.join("google_drive_api_key");
                if api_key_path.is_file() {
                    tracing::info!(
                        "Reading Google Drive api key from secret file: {:?}",
                        api_key_path
                    );
                    let google_drive_api_key = tokio::fs::read_to_string(&api_key_path)
                        .await
                        .wrap_err_with(|| {
                            format!(
                                "Error while reading Google Drive api key from secret file {:?}",
                                api_key_path
                            )
                        })?;

                    Some(SecretString::new(google_drive_api_key))
                } else {
                    None
                }
            }
            Err(unexpected) => {
                return Err(unexpected)
                    .wrap_err("Error while reading GOOGLE_DRIVE_API_KEY environment variable")
            }
        };

        Ok(Self {
            google_drive_api_key,
        })
    }
}
