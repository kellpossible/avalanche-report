use axum::extract::FromRef;
use tokio::sync::mpsc;

use crate::{
    analytics, current_weather::CurrentWeatherService, database::Database,
    forecasts::ForecastSpreadsheetSchema, i18n::I18nLoader, options::Options, templates::Templates,
};

/// App state is designed to be cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub options: &'static Options,
    pub forecast_spreadsheet_schema: &'static ForecastSpreadsheetSchema,
    pub client: reqwest::Client,
    pub i18n: I18nLoader,
    pub templates: Templates,
    pub database: Database,
    pub analytics_sx: mpsc::Sender<analytics::Event>,
    pub current_weather: std::sync::Arc<CurrentWeatherService>,
}

impl FromRef<AppState> for std::sync::Arc<CurrentWeatherService> {
    fn from_ref(state: &AppState) -> Self {
        state.current_weather.clone()
    }
}

impl FromRef<AppState> for Templates {
    fn from_ref(state: &AppState) -> Self {
        state.templates.clone()
    }
}
