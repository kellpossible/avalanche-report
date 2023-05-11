use axum::{
    body::StreamBody,
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use eyre::Context;
use http::{header::CONTENT_TYPE, StatusCode};
use once_cell::sync::Lazy;
use secrecy::SecretString;
use serde::Serialize;
use time::{OffsetDateTime, PrimitiveDateTime};
use unic_langid::LanguageIdentifier;

use crate::{error::map_eyre_error, google_drive, index::ForecastFileView, state::AppState};

static FORECAST_SCHEMA_0_3_0: Lazy<forecast_spreadsheet::options::Options> =
    Lazy::new(|| serde_json::from_str(include_str!("./schemas/0.3.0.json")).unwrap());

#[derive(Serialize, PartialEq, Eq, Clone)]
pub struct ForecastDetails {
    pub area: String,
    pub time: OffsetDateTime,
    pub forecaster: String,
}

#[derive(Clone, Serialize)]
pub struct ForecastFileDetails {
    pub forecast: ForecastDetails,
    pub language: Option<LanguageIdentifier>,
}

pub fn parse_forecast_name(file_name: &str) -> eyre::Result<ForecastFileDetails> {
    let mut name_parts = file_name.split('.');
    let details = name_parts
        .next()
        .ok_or_else(|| eyre::eyre!("File name is empty"))?;
    let mut details_split = details.split('_');
    let area = details_split
        .next()
        .ok_or_else(|| eyre::eyre!("No area specified"))?
        .to_owned();
    let time_string = details_split
        .next()
        .ok_or_else(|| eyre::eyre!("No time specified"))?;
    let format = time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]");
    let time = PrimitiveDateTime::parse(time_string, &format)
        .wrap_err_with(|| format!("Error parsing time {time_string:?}"))?
        .assume_offset(time::macros::offset!(+4));
    let forecaster = details_split
        .next()
        .ok_or_else(|| eyre::eyre!("No forecaster specified"))?
        .to_owned();

    let language = Option::transpose(
        name_parts
            .next()
            .map(|language| language.parse().wrap_err("Unable to parse language")),
    )?;

    let forecast_details = ForecastDetails {
        area,
        time,
        forecaster,
    };

    Ok(ForecastFileDetails {
        forecast: forecast_details,
        language,
    })
}

pub async fn handler(
    Path(file_name): Path<String>,
    State(state): State<AppState>,
) -> axum::response::Result<Response> {
    let api_key = state.secrets.google_drive_api_key.as_ref().ok_or_else(|| {
        tracing::error!("Unable to fetch file, Google Drive API Key not specified");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    handler_impl(&file_name, api_key, &state.client)
        .await
        .map_err(map_eyre_error)
}

async fn handler_impl(
    file_name: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<Response> {
    // Check that file exists in published folder, and not attempting to access a file outside
    // that.
    let file_list =
        google_drive::list_files("1so1EaO5clMvBUecCszKlruxnf0XpbWgr", api_key, client).await?;
    let file_metadata = match file_list
        .iter()
        .find(|file_metadata| file_metadata.name == file_name)
    {
        Some(file_metadata) => file_metadata,
        None => return Ok(StatusCode::NOT_FOUND.into_response()),
    };

    let view = match file_metadata.mime_type.as_str() {
        "application/pdf" => ForecastFileView::Download,
        "application/vnd.google-apps.spreadsheet" => ForecastFileView::Html,
        unexpected => eyre::bail!("Unsupported file mime type {unexpected}"),
    };

    match view {
        ForecastFileView::Html => {
            let file = google_drive::export_file(
                &file_metadata.id,
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                api_key,
                client,
            )
            .await?;
            let bytes = file.bytes().await?;

            let forecast =
                forecast_spreadsheet::parse_excel_spreadsheet(&bytes, &*FORECAST_SCHEMA_0_3_0)
                    .context("Error parsing forecast spreadsheet")?;
            Ok(axum::response::Json(forecast).into_response())
        }
        ForecastFileView::Download => {
            let file = google_drive::get_file(&file_metadata.id, api_key, client).await?;
            let builder = Response::builder();
            let builder = match file.content_type() {
                Some(content_type) => builder.header(CONTENT_TYPE, content_type),
                None => builder,
            };

            let body = StreamBody::new(file.bytes_stream());
            let response = builder.body(body)?.into_response();
            Ok(response)
        }
    }
}

#[cfg(test)]
mod test {
    use super::parse_forecast_name;

    #[test]
    fn test_parse_forecast_name() {
        let forecast_details = parse_forecast_name("Gudauri_2023-01-24T17:00_LF.pdf").unwrap();
        insta::assert_json_snapshot!(forecast_details, @r###"
        {
          "forecast": {
            "area": "Gudauri",
            "time": [
              2023,
              24,
              17,
              0,
              0,
              0,
              4,
              0,
              0
            ],
            "forecaster": "LF"
          },
          "language": "pdf"
        }
        "###);
    }
}
