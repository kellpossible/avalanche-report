use axum::{extract::State, response::IntoResponse, Extension};
use color_eyre::Help;
use eyre::Context;
use http::StatusCode;
use serde::Serialize;
use unic_langid::LanguageIdentifier;

use crate::{
    error::map_eyre_error,
    forecasts::{parse_forecast_name, ForecastDetails, ForecastFileDetails},
    google_drive::{self, ListFileMetadata},
    i18n::I18nLoader,
    state::AppState,
    templates::{render, TemplatesWithContext},
};

fn format_language_name(language: &LanguageIdentifier) -> Option<String> {
    match language.language.as_str() {
        "en" => Some("English".to_owned()),
        "ka" => Some("ქართული".to_owned()),
        _ => None,
    }
}

#[derive(Clone, Serialize, Debug)]
pub enum ForecastFileView {
    /// Forecast file is viewed by being parsed and rendered as HTML (Spreadsheet).
    Html,
    /// Forecast file is downloaded to be viewed (PDF).
    Download,
}

#[derive(Clone, Serialize, Debug)]
pub struct ForecastFile {
    pub details: FormattedForecastFileDetails,
    pub file: ListFileMetadata,
    pub view: ForecastFileView,
}

#[derive(Clone, Serialize, Debug)]
pub struct FormattedForecastFileDetails {
    pub forecast: FormattedForecastDetails,
    pub language: Option<String>,
}

impl FormattedForecastFileDetails {
    fn format(value: ForecastFileDetails, i18n: &I18nLoader) -> Self {
        Self {
            forecast: FormattedForecastDetails::format(value.forecast, i18n),
            language: value.language.map(|language| {
                format_language_name(&language).unwrap_or_else(|| language.to_string())
            }),
        }
    }
}

#[derive(PartialEq, Clone, Serialize, Debug)]
pub struct FormattedForecastDetails {
    pub area: String,
    pub formatted_time: String,
    pub time: time::OffsetDateTime,
    pub forecaster: String,
}

impl FormattedForecastDetails {
    fn format(value: ForecastDetails, i18n: &I18nLoader) -> Self {
        let day = value.time.day();
        let month = value.time.month() as u8;
        let month_name = i18n.get(&format!("month-{month}"));
        let year = value.time.year();
        let hour = value.time.hour();
        let minute = value.time.minute();
        let formatted_time = format!("{day} {month_name} {year} {hour:0>2}:{minute:0>2}");
        Self {
            area: value.area,
            formatted_time,
            time: value.time,
            forecaster: value.forecaster,
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Forecast {
    pub details: FormattedForecastDetails,
    pub files: Vec<ForecastFile>,
}

#[derive(Serialize, Debug)]
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
        .map(|file| {
            let filename = &file.name;
            let details: ForecastFileDetails = parse_forecast_name(filename).wrap_err_with(|| {
                    eyre::eyre!("Error parsing forecast details from file {filename:?}")
                })
                .suggestion("Name file according to the standard format.\n e.g. \"Gudauri_2023-01-24T17:00_LF.en.pdf\"")?;
            match details.forecast.area.as_str() {
                "Gudauri" => (),
                unknown => return Err(
                    eyre::eyre!(
                        "Unknown forecast area {unknown:?} in filename \
                        {filename:?}"
                    ))
                    .suggestion(
                        "Forecast area name is case sensitive. \
                        Available forecast areas: Gudauri"
                    )
            }
            let formatted_details = FormattedForecastFileDetails::format(details, &i18n);

            let filename = &file.name;
            let view = match file.mime_type.as_str() {
                "application/pdf" => ForecastFileView::Download,
                "application/vnd.google-apps.spreadsheet" => ForecastFileView::Html,
                unsupported => eyre::bail!("Unsupported mime {unsupported} for file {filename}"),
            };
            Ok(ForecastFile { details: formatted_details, file, view })
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
    render(&templates.environment, "index.html", &index).map_err(map_eyre_error)
}
