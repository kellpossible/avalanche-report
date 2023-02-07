use std::env::VarError;

use eyre::Context;
use secrecy::SecretString;

/// Secrets configuration. These are values that should not be exposed publicly. Separate from
/// [crate::options::Options] in order to support loading from separate environment variables in
/// deployment situations that support hidden/secret/protected variables.
pub struct Secrets {
    pub google_drive_api_key: Option<SecretString>,
}

impl Secrets {
    pub fn initialize() -> eyre::Result<Self> {
        let google_drive_api_key = match std::env::var("GOOGLE_DRIVE_API_KEY") {
            Ok(google_drive_api_key) => {
                tracing::info!(
                    "Google Drive api key was read from GOOGLE_DRIVE_API_KEY environment variable"
                );
                Some(SecretString::new(google_drive_api_key))
            }
            Err(VarError::NotPresent) => None,
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
