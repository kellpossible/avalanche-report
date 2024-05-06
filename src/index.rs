use std::sync::Arc;

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Json,
};
use color_eyre::Help;
use eyre::{bail, eyre, Context, ContextCompat};
use forecast_spreadsheet::{HazardRating, HazardRatingKind};
use futures::{stream, StreamExt, TryStreamExt};
use headers::{CacheControl, ContentType, HeaderMapExt};
use i18n_embed::{fluent::FluentLanguageLoader, LanguageLoader};
use indexmap::IndexMap;
use serde::Serialize;
use unic_langid::LanguageIdentifier;

use crate::{
    database::Database,
    error::map_eyre_error,
    forecasts::{
        get_forecast_data, parse_forecast_name, Forecast, ForecastContext, ForecastData,
        ForecastDetails, ForecastFileDetails, RequestedForecastData,
    },
    google_drive::{self, ListFileMetadata},
    i18n::{self, I18nLoader},
    options::{WeatherMaps, WeatherStationId},
    state::AppState,
    templates::{render, TemplatesWithContext},
    user_preferences::{UserPreferences, WindUnit},
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

#[derive(Clone, Debug)]
pub struct ForecastFile {
    pub details: FormattedForecastFileDetails,
    pub file: ListFileMetadata,
}

#[derive(Clone, Serialize, Debug)]
pub struct ForecastFileContext {
    path: String,
}

impl From<ForecastFile> for ForecastFileContext {
    fn from(value: ForecastFile) -> Self {
        let path = format!("/forecast/{}", urlencoding::encode(&value.file.name));
        Self { path }
    }
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
    #[serde(with = "time::serde::rfc3339")]
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
pub struct IndexFullForecastContext {
    pub details: FormattedForecastDetails,
    pub file: ForecastFileContext,
    pub forecast: Option<ForecastContext>,
}

#[derive(Serialize, Debug, Clone)]
pub struct IndexSummaryForecastContext {
    pub details: FormattedForecastDetails,
    pub file: ForecastFileContext,
    pub hazard_ratings: IndexMap<HazardRatingKind, HazardRating>,
}

impl From<IndexFullForecastContext> for IndexSummaryForecastContext {
    fn from(forecast: IndexFullForecastContext) -> Self {
        Self {
            details: forecast.details,
            file: forecast.file,
            hazard_ratings: forecast
                .forecast
                .map(|forecast| forecast.forecast.hazard_ratings)
                .unwrap_or_default(),
        }
    }
}

pub struct ForecastAccumulator {
    pub details: FormattedForecastDetails,
    pub files: Vec<ForecastFile>,
}

#[derive(Serialize, Debug)]
struct WeatherContext {
    wind_unit: WindUnit,
    weather_maps: WeatherMaps,
    weather_station_ids: Vec<WeatherStationId>,
}

#[derive(Serialize, Debug)]
struct IndexContext {
    current_forecast: Option<IndexFullForecastContext>,
    forecasts: Vec<IndexSummaryForecastContext>,
    errors: Vec<String>,
    weather: WeatherContext,
}

pub async fn handler(
    Extension(templates): Extension<TemplatesWithContext>,
    Extension(i18n): Extension<I18nLoader>,
    Extension(database): Extension<Database>,
    Extension(preferences): Extension<UserPreferences>,
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> axum::response::Result<Response> {
    let requested_content_type = request.headers().typed_get::<headers::ContentType>();
    Ok(handler_impl(
        requested_content_type,
        templates,
        i18n,
        database,
        preferences,
        state,
    )
    .await
    .map_err(map_eyre_error)?)
}

/// Handler for requests that should always return JSON.
pub async fn json_handler(
    Extension(templates): Extension<TemplatesWithContext>,
    Extension(i18n): Extension<I18nLoader>,
    Extension(database): Extension<Database>,
    Extension(preferences): Extension<UserPreferences>,
    State(state): State<AppState>,
) -> axum::response::Result<Response> {
    Ok(handler_impl(
        Some(ContentType::json()),
        templates,
        i18n,
        database,
        preferences,
        state,
    )
    .await
    .map_err(map_eyre_error)?)
}

async fn handler_impl(
    requested_content_type: Option<ContentType>,
    templates: TemplatesWithContext,
    i18n: Arc<FluentLanguageLoader>,
    database: Database,
    preferences: UserPreferences,
    state: AppState,
) -> eyre::Result<Response> {
    let file_list = google_drive::list_files(
        &state.options.google_drive.published_folder_id,
        &state.options.google_drive.api_key,
        &state.client,
    )
    .await
    .wrap_err("Error listing google drive files")?;
    let (forecasts, mut errors): (Vec<ForecastAccumulator>, Vec<String>) = file_list
        .iter()
        .map(|file| {
            let filename = &file.name;
            let details: ForecastFileDetails = parse_forecast_name(filename, state.forecast_spreadsheet_schema).wrap_err_with(|| {
                    eyre!("Error parsing forecast details from file {filename:?}")
                })
                .suggestion("Name file according to the standard format.\n e.g. \"Gudauri_2023-01-24T17:00_LF.en.pdf\"")?;
            match details.forecast.area.as_str() {
                "Gudauri" => (),
                "Bansko" => (),
                unknown => return Err(
                    eyre!(
                        "Unknown forecast area {unknown:?} in filename \
                        {filename:?}"
                    ))
                    .suggestion(
                        "Forecast area name is case sensitive. \
                        Available forecast areas: Gudauri, Bansko"
                    )
            }
            let formatted_details = FormattedForecastFileDetails::format(details, &i18n);

            let filename = &file.name;
            let view = match file.mime_type.as_str() {
                "application/pdf" => ForecastFileView::Download,
                "application/vnd.google-apps.spreadsheet" => ForecastFileView::Html,
                unsupported => bail!("Unsupported mime {unsupported} for file {filename}"),
            };
            Ok(ForecastFile { details: formatted_details, file: file.clone() })
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

    let (mut forecasts, errors_2): (Vec<IndexFullForecastContext>, Vec<String>) =
        stream::iter(forecasts)
            .map::<eyre::Result<ForecastAccumulator>, _>(eyre::Result::Ok)
            .and_then(|forecast_acc| async {
                tracing::debug!("forecast_acc.files {:?}", forecast_acc.files);
                let file: ForecastFile = forecast_acc
                    .files
                    .iter()
                    .filter(|file| {
                        if let Some(language) = &file.details.language {
                            return language.language == i18n.current_language().language;
                        }
                        true
                    })
                    .cloned()
                    .next()
                    .or_else(|| forecast_acc.files.into_iter().next())
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
                        &state.forecast_spreadsheet_schema,
                    )
                    .await?
                    {
                        ForecastData::Forecast(forecast) => {
                            let forecast = Forecast::try_new(forecast)?;
                            let formatted_forecast: ForecastContext = ForecastContext::format(
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

                eyre::Result::Ok(IndexFullForecastContext {
                    details: forecast_acc.details,
                    file: file.into(),
                    forecast,
                })
            })
            .map_err(|error| error.wrap_err("Error converting accumulated forecast"))
            .fold((Vec::new(), Vec::new()), |mut acc, result| async move {
                match result {
                    Ok(ok) => acc.0.push(ok),
                    Err(error) => acc.1.push(format!("{error:?}")),
                }
                acc
            })
            .await;

    errors.extend(errors_2.into_iter());

    forecasts.sort_by(|a, b| b.details.time.cmp(&a.details.time));

    let current_forecast = forecasts.first().and_then(|forecast| {
        let f = &forecast.forecast.as_ref()?.forecast;
        if f.is_current() {
            Some(forecast.clone())
        } else {
            None
        }
    });

    let forecasts = forecasts
        .into_iter()
        .map(IndexSummaryForecastContext::from)
        .collect();

    let index = IndexContext {
        current_forecast,
        forecasts,
        errors,
        weather: WeatherContext {
            wind_unit: preferences.wind_unit.unwrap_or_default(),
            weather_station_ids: state.options.weather_stations.keys().cloned().collect(),
            weather_maps: state.options.weather_maps.clone(),
        },
    };

    if requested_content_type == Some(ContentType::json()) {
        Ok(Json(index).into_response())
    } else {
        let mut response = render(&templates.environment, "index.html", &index)?.into_response();
        response
            .headers_mut()
            .typed_insert(CacheControl::new().with_no_store());
        Ok(response)
    }
}
