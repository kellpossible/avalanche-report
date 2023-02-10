use std::env::VarError;

use color_eyre::Help;
use eyre::Context;
use secrecy::SecretString;

/// Secrets configuration. These are values that should not be exposed publicly. Separate from
/// [crate::options::Options] in order to support loading from separate environment variables in
/// deployment situations that support hidden/secret/protected variables.
pub struct Secrets {
    pub google_drive_api_key: Option<SecretString>,
    pub admin_password_hash: SecretString,
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

        let admin_password_hash = match std::env::var("ADMIN_PASSWORD_HASH") {
            Ok(admin_password_hash) => {
                tracing::info!(
                    "Admin password hash was read from ADMIN_PASSWORD_HASH environment variable"
                );
                SecretString::new(admin_password_hash)
            }
            Err(unexpected) => {
                return Err(unexpected)
                    .wrap_err("Error while reading ADMIN_PASSWORD_HASH environment variable")
                    .suggestion(
                        "ADMIN_PASSWORD_HASH environment variable must be set to run this application. \
                        You can use the admin-password-hash binary (subproject of this one) to generate \
                        the hash of your desired password.
                        "
                    )
            }
        };

        Ok(Self {
            google_drive_api_key,
            admin_password_hash,
        })
    }
}
