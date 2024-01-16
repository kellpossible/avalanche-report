use axum::{extract::State, response::IntoResponse, Extension};
use color_eyre::Help;
use eyre::{bail, eyre, Context, ContextCompat};
use futures::{stream, StreamExt, TryStreamExt};
use i18n_embed::LanguageLoader;
use serde::Serialize;
use unic_langid::LanguageIdentifier;

use crate::{
    database::DatabaseInstance,
    error::map_eyre_error,
    forecasts::{
        get_forecast_data, parse_forecast_name, Forecast, ForecastData, ForecastDetails,
        ForecastFileDetails, FormattedForecast, RequestedForecastData,
    },
    google_drive::{self, ListFileMetadata},
    i18n::{self, I18nLoader},
    state::AppState,
    templates::{render, TemplatesWithContext},
    user_preferences::UserPreferences,
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

#[derive(Serialize, Debug, Clone)]
pub struct IndexForecast {
    pub details: FormattedForecastDetails,
    pub file: ForecastFile,
    pub forecast: Option<FormattedForecast>,
}

pub struct ForecastAccumulator {
    pub details: FormattedForecastDetails,
    pub files: Vec<ForecastFile>,
}

#[derive(Serialize, Debug)]
struct Index {
    current_forecast: Option<IndexForecast>,
    forecasts: Vec<IndexForecast>,
    errors: Vec<String>,
}

pub async fn handler(
    Extension(templates): Extension<TemplatesWithContext>,
    Extension(i18n): Extension<I18nLoader>,
    Extension(database): Extension<DatabaseInstance>,
    Extension(preferences): Extension<UserPreferences>,
    State(state): State<AppState>,
) -> axum::response::Result<impl IntoResponse> {
    let file_list = google_drive::list_files(
        &state.options.google_drive.published_folder_id,
        &state.options.google_drive.api_key,
        &state.client,
    )
    .await
    .wrap_err("Error listing google drive files")
    .map_err(map_eyre_error)?;
    let (forecasts, errors): (Vec<ForecastAccumulator>, Vec<String>) = file_list
        .iter()
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
            Ok(ForecastFile { details: formatted_details, file: file.clone(), view })
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

    let mut forecasts: Vec<IndexForecast> = stream::iter(forecasts)
        .map::<eyre::Result<ForecastAccumulator>, _>(eyre::Result::Ok)
        .and_then(|forecast_acc| async {
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

            let forecast = if file.file.is_google_sheet() {
                match get_forecast_data(
                    &file.file,
                    RequestedForecastData::Forecast,
                    &state.client,
                    &database,
                    &state.options.google_drive.api_key,
                )
                .await?
                {
                    ForecastData::Forecast(forecast) => {
                        let forecast = Forecast::try_new(forecast)?;
                        let formatted_forecast: FormattedForecast = FormattedForecast::format(
                            forecast,
                            &i18n,
                            &state.options,
                            &preferences,
                        );
                        Some(formatted_forecast)
                    }
                    ForecastData::File(_) => {
                        return Err(eyre::eyre!("Expected ForecastData::Forecast").into())
                    }
                }
            } else {
                None
            };

            eyre::Result::Ok(IndexForecast {
                details: forecast_acc.details,
                file,
                forecast,
            })
        })
        .try_collect()
        .await
        .wrap_err("Error converting accumulated forecast")
        .map_err(map_eyre_error)?;

    forecasts.sort_by(|a, b| b.details.time.cmp(&a.details.time));

    let current_forecast = forecasts.first().and_then(|forecast| {
        let f = &forecast.forecast.as_ref()?.forecast;
        if f.is_current() {
            Some(forecast.clone())
        } else {
            None
        }
    });

    let index = Index {
        current_forecast,
        forecasts,
        errors,
    };
    render(&templates.environment, "index.html", &index).map_err(map_eyre_error)
}
