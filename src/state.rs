use axum::extract::FromRef;

use crate::{database::Database, i18n::I18nLoader, secrets::Secrets, templates::Templates};

/// App state is designed to be cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub secrets: &'static Secrets,
    pub client: reqwest::Client,
    pub i18n: I18nLoader,
    pub templates: Templates,
    pub database: Database,
}

impl FromRef<AppState> for Templates {
    fn from_ref(state: &AppState) -> Self {
        state.templates.clone()
    }
}
