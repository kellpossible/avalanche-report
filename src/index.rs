use axum::{extract::State, response::IntoResponse, Extension};
use color_eyre::Help;
use eyre::{bail, eyre, Context, ContextCompat};
use i18n_embed::LanguageLoader;
use serde::Serialize;
use unic_langid::LanguageIdentifier;

use crate::{
    error::map_eyre_error,
    forecasts::{parse_forecast_name, ForecastDetails, ForecastFileDetails},
    google_drive::{self, ListFileMetadata},
    i18n::{self, I18nLoader},
    state::AppState,
    templates::{render, TemplatesWithContext},
};

#[derive(Clone, Serialize, Debug)]
pub enum ForecastFileView {
    /// Forecast file is viewed by being parsed and rendered as HTML.
    Html,
    /// Forecast file is viewed by being parsed, and serialized to JSON.
    Json,
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
    pub language: Option<LanguageIdentifier>,
}

impl FormattedForecastFileDetails {
    fn format(value: ForecastFileDetails, i18n: &I18nLoader) -> Self {
        Self {
            forecast: FormattedForecastDetails::format(value.forecast, i18n),
            language: value.language,
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
    fn format(details: ForecastDetails, i18n: &I18nLoader) -> Self {
        let formatted_time = i18n::format_time(details.time, i18n);
        Self {
            area: details.area,
            formatted_time,
            time: details.time,
            forecaster: details.forecaster,
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Forecast {
    pub details: FormattedForecastDetails,
    pub file: ForecastFile,
}

pub struct ForecastAccumulator {
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
    let files = google_drive::list_files(
        "1so1EaO5clMvBUecCszKlruxnf0XpbWgr",
        &state.options.google_drive_api_key,
        &state.client,
    )
    .await
    .wrap_err("Error listing google drive files")
    .map_err(map_eyre_error)?;
    let (forecasts, errors): (Vec<ForecastAccumulator>, Vec<String>) = files
        .into_iter()
        .map(|file| {
            let filename = &file.name;
            let details: ForecastFileDetails = parse_forecast_name(filename).wrap_err_with(|| {
                    eyre!("Error parsing forecast details from file {filename:?}")
                })
                .suggestion("Name file according to the standard format.\n e.g. \"Gudauri_2023-01-24T17:00_LF.en.pdf\"")?;
            match details.forecast.area.as_str() {
                "Gudauri" => (),
                unknown => return Err(
                    eyre!(
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
                unsupported => bail!("Unsupported mime {unsupported} for file {filename}"),
            };
            Ok(ForecastFile { details: formatted_details, file, view })
        })
        .fold((Vec::new(), Vec::new()), |mut acc, result: eyre::Result<ForecastFile>| {
            match result {
                Ok(forecast_file) => {
                    if let Some(i) = acc.0.iter().position(|forecast| forecast.details == forecast_file.details.forecast) {

                        let forecast_acc = acc.0.get_mut(i).unwrap();
                        forecast_acc.files.push(forecast_file);

                    } else {
                        acc.0.push(ForecastAccumulator { details: forecast_file.details.forecast.clone(), files: vec![forecast_file] });
                    }
                }
                Err(error) => acc.1.push(format!("{error:#?}")),
            }
            acc
        });

    let mut forecasts: Vec<Forecast> = forecasts
        .into_iter()
        .map(|forecast_acc| {
            let file: ForecastFile = if forecast_acc.files.len() > 1 {
                forecast_acc
                    .files
                    .into_iter()
                    .filter(|file| {
                        if let Some(language) = &file.details.language {
                            return language.language == i18n.current_language().language;
                        }
                        true
                    })
                    .next()
            } else {
                forecast_acc.files.into_iter().next()
            }
            .wrap_err_with(|| {
                format!(
                    "Expected there to be at least one forecast file for this forecast {:#?}",
                    forecast_acc.details
                )
            })?;

            Ok(Forecast {
                details: forecast_acc.details,
                file,
            })
        })
        .collect::<eyre::Result<_>>()
        .wrap_err("Error converting accumulated forecast")
        .map_err(map_eyre_error)?;

    forecasts.sort_by(|a, b| b.details.time.cmp(&a.details.time));

    let index = Index { forecasts, errors };
    render(&templates.environment, "index.html", &index).map_err(map_eyre_error)
}
