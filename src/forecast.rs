use axum::{extract::{State, Path}, response::{IntoResponse, Response}, body::StreamBody};
use time::{OffsetDateTime, PrimitiveDateTime};
use eyre::Context;
use http::{StatusCode, header::CONTENT_TYPE};
use secrecy::SecretString;
use serde::Serialize;
use unic_langid::LanguageIdentifier;

use crate::{error::map_eyre_error, state::AppState, google_drive};

#[derive(Serialize, PartialEq, Eq, Clone)]
pub struct ForecastDetails {
    pub area: String,
    pub time: OffsetDateTime,
    pub forecaster: String,
}

#[derive(Clone, Serialize)]
pub struct ForecastFileDetails {
    pub forecast: ForecastDetails,
    pub language: LanguageIdentifier,
}



pub fn parse_forecast_details(file_name: &str) -> eyre::Result<ForecastFileDetails> {
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
    let time = PrimitiveDateTime::parse(&time_string, &format)
        .wrap_err_with(|| format!("Error parsing time {time_string:?}"))?
        .assume_offset(time::macros::offset!(+4));
    let forecaster = details_split
        .next()
        .ok_or_else(|| eyre::eyre!("No forecaster specified"))?
        .to_owned();

    let language = name_parts
        .next()
        .ok_or_else(|| eyre::eyre!("No language specified"))?
        .parse()
        .wrap_err("Unable to parse language")?;

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
    Path(file_id): Path<String>,
    State(state): State<AppState>,
) -> axum::response::Result<impl IntoResponse> {
    let google_drive_api_key = state.secrets.google_drive_api_key.as_ref().ok_or_else(|| {
        tracing::error!("Unable to fetch file, Google Drive API Key not specified");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(
        handler_impl(&file_id, google_drive_api_key, &state.client)
            .await
            .map_err(map_eyre_error)?,
    )
}

async fn handler_impl(
    file_id: &str,
    google_drive_api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<impl IntoResponse> {
    let file = google_drive::get_file(&file_id, google_drive_api_key, client).await?;
    let builder = Response::builder();
    let builder = match file.content_type() {
        Some(content_type) => builder.header(CONTENT_TYPE, content_type),
        None => builder,
    };

    let body = StreamBody::new(file.bytes_stream());
    let response = builder.body(body)?;
    Ok(response)
}

#[cfg(test)]
mod test {
    use super::parse_forecast_details;

    #[test]
    fn test_parse_forecast_details() {
        let forecast_details = parse_forecast_details("Gudauri_2023-01-24T17:00_LF.pdf").unwrap();
        insta::assert_json_snapshot!(forecast_details, @r###"
        {
          "area": "Gudauri",
          "time": "2023-01-24T17:00:00+04"
        }
        "###);
    }
}
