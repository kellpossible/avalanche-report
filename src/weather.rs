use axum::{extract::State, response::IntoResponse, Extension};
use serde::{Deserialize, Serialize};

use crate::{
    error::map_eyre_error,
    state::AppState,
    templates::{render, TemplatesWithContext},
    user_preferences::{self, UserPreferences, WindUnit},
};

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Query {
    wind_unit: Option<WindUnit>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Context {
    weather_maps: crate::options::WeatherMaps,
    wind_unit: WindUnit,
}

impl Context {
    pub fn new(options: &crate::Options, preferences: &UserPreferences) -> Self {
        Self {
            weather_maps: options.weather_maps.clone(),
            wind_unit: preferences.wind_unit.unwrap_or_default(),
        }
    }
}

pub async fn handler(
    axum::extract::Query(query): axum::extract::Query<Query>,
    State(state): State<AppState>,
    Extension(templates): Extension<TemplatesWithContext>,
    Extension(current_preferences): Extension<UserPreferences>,
) -> axum::response::Result<impl IntoResponse> {
    let set_preferences = UserPreferences {
        wind_unit: query.wind_unit,
        ..UserPreferences::default()
    };
    let set_preferences_cookie =
        user_preferences::set_preferences_cookie(set_preferences, current_preferences)
            .map_err(map_eyre_error)?;
    let context = Context::new(state.options, &set_preferences_cookie.new_preferences);
    let mut response =
        render(&templates.environment, "weather.html", &context).map_err(map_eyre_error)?;

    set_preferences_cookie.set_cookie(response.headers_mut());
    Ok(response)
}
