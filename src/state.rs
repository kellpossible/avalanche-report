use axum::extract::FromRef;

use crate::{i18n::I18nLoader, secrets::Secrets, templates::Templates};

#[derive(Clone)]
pub struct AppState {
    pub secrets: &'static Secrets,
    pub client: reqwest::Client,
    pub i18n: I18nLoader,
    pub templates: Templates,
}

impl FromRef<AppState> for Templates {
    fn from_ref(state: &AppState) -> Self {
        state.templates.clone()
    }
}
