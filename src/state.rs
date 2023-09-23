use axum::extract::FromRef;
use tokio::sync::mpsc;

use crate::{
    analytics, database::Database, i18n::I18nLoader, options::Options, templates::Templates,
};

/// App state is designed to be cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub options: &'static Options,
    pub client: reqwest::Client,
    pub i18n: I18nLoader,
    pub templates: Templates,
    pub database: Database,
    pub analytics_sx: mpsc::Sender<analytics::Event>,
}

impl FromRef<AppState> for Templates {
    fn from_ref(state: &AppState) -> Self {
        state.templates.clone()
    }
}
