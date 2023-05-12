use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Extension,
};
use eyre::Context;
use http::{header::CONTENT_TYPE, HeaderValue, StatusCode};
use once_cell::sync::Lazy;
use sea_query::{Alias, Expr, IntoColumnRef, OnConflict, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;
use secrecy::SecretString;
use serde::Serialize;
use time::{OffsetDateTime, PrimitiveDateTime};
use tracing::instrument;
use unic_langid::LanguageIdentifier;

use crate::{
    database::DatabaseInstance, error::map_eyre_error, google_drive, index::ForecastFileView,
    state::AppState,
};

use self::files::{ForecastFiles, ForecastFilesIden};

mod files;

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
    Extension(database): Extension<DatabaseInstance>,
) -> axum::response::Result<Response> {
    let api_key = state.secrets.google_drive_api_key.as_ref().ok_or_else(|| {
        tracing::error!("Unable to fetch file, Google Drive API Key not specified");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    handler_impl(&file_name, api_key, &state.client, &database)
        .await
        .map_err(map_eyre_error)
}

#[instrument(level = "error", skip_all)]
async fn handler_impl(
    file_name: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
    database: &DatabaseInstance,
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

    let google_drive_id = file_metadata.id.clone();
    let cached_forecast_file: Option<Vec<u8>> = database
        .interact(move |conn| {
            let mut query = sea_query::Query::select();

            query
                .columns(ForecastFiles::COLUMNS)
                .from(ForecastFiles::TABLE)
                .and_where(Expr::col(ForecastFilesIden::GoogleDriveId).eq(&google_drive_id));

            let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
            let mut statement = conn.prepare_cached(&sql)?;

            let forecast_file = Option::transpose(
                statement
                    .query_map(&*values.as_params(), |row| ForecastFiles::try_from(row))
                    .wrap_err("Error performing query to obtain `ForecastFile`")?
                    .next(),
            )?;
            Ok::<_, eyre::Error>(forecast_file)
        })
        .await??
        .and_then(|cached_forecast_file| {
            let cached_last_modified: OffsetDateTime = cached_forecast_file.last_modified.into();
            let server_last_modified: &OffsetDateTime = &file_metadata.modified_time;
            tracing::debug!("cached last modified {cached_last_modified}, server last modified {server_last_modified}");
            if cached_last_modified == *server_last_modified {
                Some(cached_forecast_file.file_blob)
            } else {
                tracing::debug!("Found cached forecast file, but it's outdated");
                None
            }
        });

    let forecast_file_bytes: Vec<u8> = if let Some(cached_forecast_file) = cached_forecast_file {
        tracing::debug!("Using cached forecast file");
        cached_forecast_file
    } else {
        tracing::debug!("Fetching updated/new forecast file");
        let forecast_file_bytes: Vec<u8> = match view {
            ForecastFileView::Html => {
                let file = google_drive::export_file(
                    &file_metadata.id,
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                    api_key,
                    client,
                )
                .await?;
                file.bytes().await?.into()
            }
            ForecastFileView::Download => {
                let file = google_drive::get_file(&file_metadata.id, api_key, client).await?;
                file.bytes().await?.into()
            }
        };
        let forecast_files_db = ForecastFiles {
            google_drive_id: file_metadata.id.clone(),
            last_modified: file_metadata.modified_time.clone().into(),
            file_blob: forecast_file_bytes.clone(),
        };
        database
            .interact(move |conn| {
                let mut query = sea_query::Query::insert();

                let values = forecast_files_db.values();
                query
                    .into_table(ForecastFiles::TABLE)
                    .columns(ForecastFiles::COLUMNS)
                    .values(values)?;

                let excluded_table: Alias = Alias::new("excluded");
                query.on_conflict(
                    OnConflict::column(ForecastFilesIden::GoogleDriveId)
                        .values([
                            (
                                ForecastFilesIden::LastModified,
                                (excluded_table.clone(), ForecastFilesIden::LastModified)
                                    .into_column_ref()
                                    .into(),
                            ),
                            (
                                ForecastFilesIden::FileBlob,
                                (excluded_table, ForecastFilesIden::FileBlob)
                                    .into_column_ref()
                                    .into(),
                            ),
                        ])
                        .to_owned(),
                );

                let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
                conn.execute(&sql, &*values.as_params())?;
                Result::<_, eyre::Error>::Ok(())
            })
            .await??;
        forecast_file_bytes
    };

    match view {
        ForecastFileView::Html => {
            let forecast = forecast_spreadsheet::parse_excel_spreadsheet(
                &forecast_file_bytes,
                &*FORECAST_SCHEMA_0_3_0,
            )
            .context("Error parsing forecast spreadsheet")?;
            Ok(axum::response::Json(forecast).into_response())
        }
        ForecastFileView::Download => {
            let mut response = forecast_file_bytes.into_response();
            let header_value = HeaderValue::from_str(&file_metadata.mime_type)?;
            response.headers_mut().insert(CONTENT_TYPE, header_value);
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
