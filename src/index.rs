use axum::{
    extract::State,
    response::IntoResponse,
    Extension,
};
use color_eyre::Help;
use eyre::Context;
use http::StatusCode;
use serde::Serialize;
use unic_langid::LanguageIdentifier;

use crate::{
    error::{map_eyre_error, map_std_error},
    forecast::{parse_forecast_details, ForecastDetails, ForecastFileDetails},
    google_drive::{self, FileMetadata},
    state::AppState,
    templates::TemplatesWithContext, i18n::I18nLoader,
};

fn format_language_name(language: &LanguageIdentifier) -> Option<String> {
    match language.language.as_str() {
        "en" => Some("English".to_owned()),
        "ka" => Some("ქართული".to_owned()),
        _ => None,
    }
}

#[derive(Clone, Serialize)]
pub struct ForecastFile {
    pub details: FormattedForecastFileDetails,
    pub file: FileMetadata,
}

#[derive(Clone, Serialize)]
pub struct FormattedForecastFileDetails {
    pub forecast: FormattedForecastDetails,
    pub language: String,
}

impl FormattedForecastFileDetails {
    fn format(value: ForecastFileDetails, i18n: &I18nLoader) -> Self {
        Self {
            forecast: FormattedForecastDetails::format(value.forecast, i18n),
            language: format_language_name(&value.language).unwrap_or_else(|| value.language.to_string()),
        }
    }
}

#[derive(PartialEq, Clone, Serialize)]
pub struct FormattedForecastDetails {
    pub area: String,
    pub time: String,
    pub forecaster: String,
}

impl FormattedForecastDetails {
    fn format(value: ForecastDetails, _i18n: &I18nLoader) -> Self {
        let day = value.time.day();
        let month = value.time.month();
        let year = value.time.year();
        let hour = value.time.hour();
        let minute = value.time.minute();
        let time = format!("{day} {month} {year} {hour:0>2}:{minute:0>2}");
        Self {
            area: value.area,
            time,
            forecaster: value.forecaster,
        }
    }
}

#[derive(Serialize)]
pub struct Forecast {
    pub details: FormattedForecastDetails,
    pub files: Vec<ForecastFile>,
}

#[derive(Serialize)]
struct Index {
    forecasts: Vec<Forecast>,
    errors: Vec<String>,
}

pub async fn handler(
    Extension(templates): Extension<TemplatesWithContext>,
    Extension(i18n): Extension<I18nLoader>,
    State(state): State<AppState>,
) -> axum::response::Result<impl IntoResponse> {
    let files = match &state.secrets.google_drive_api_key {
        Some(api_key) => {
            google_drive::list_files("1so1EaO5clMvBUecCszKlruxnf0XpbWgr", api_key, &state.client)
                .await
                .wrap_err("Error listing google drive files")
                .map_err(map_eyre_error)?
        }
        None => {
            tracing::warn!("Unable to list files, no Google Drive api key secret is specified");
            return Ok(StatusCode::INTERNAL_SERVER_ERROR.into_response());
        }
    };
    let (mut forecasts, errors): (Vec<Forecast>, Vec<String>) = files
        .into_iter()
        .filter(|file| file.mime_type == "application/pdf")
        .map(|file| {
            let filename = &file.name;
            let details = parse_forecast_details(filename).wrap_err_with(|| {
                    eyre::eyre!("Error parsing forecast details from file {filename:?}")
                })
                .suggestion("Name file according to the standard format.\n e.g. \"Gudauri_2023-01-24T17:00_LF.en.pdf\"")?;
            let formatted_details = FormattedForecastFileDetails::format(details, &i18n);
            Ok(ForecastFile { details: formatted_details, file })
        })
        .fold((Vec::new(), Vec::new()), |mut acc, result: eyre::Result<ForecastFile>| {
            match result {
                Ok(forecast_file) => {
                    if let Some(i) = acc.0.iter().position(|forecast| forecast.details == forecast_file.details.forecast) {
                        acc.0.get_mut(i).unwrap().files.push(forecast_file)
                    } else {
                        acc.0.push(Forecast { details: forecast_file.details.forecast.clone(), files: vec![forecast_file] });
                    }
                }
                Err(error) => acc.1.push(format!("{error:?}")),
            }
            acc
        });

    forecasts.sort_by(|a, b| b.details.time.cmp(&a.details.time));

    let index = Index { forecasts, errors };

    let template = templates
        .environment
        .get_template("index.html")
        .map_err(map_std_error)?;
    Ok(crate::templates::render(&template, &index).into_response())
}
