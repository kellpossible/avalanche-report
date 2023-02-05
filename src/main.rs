use axum::{
    body::{boxed, Full, StreamBody},
    extract::{Path, State},
    handler::HandlerWithoutStateExt,
    http::{header, StatusCode, Uri},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
    Extension, Router,
};
use chrono::{DateTime, NaiveDateTime};
use color_eyre::Help;
use error::handle_eyre_error;
use eyre::Context;
use google_drive::FileMetadata;
use html_builder::{Html5, Node};
use http::header::CONTENT_TYPE;
use i18n::I18nLoader;
use rust_embed::RustEmbed;
use secrecy::SecretString;
use serde::Serialize;
use std::{fmt::Write, marker::PhantomData};
use tracing_appender::rolling::Rotation;
use unic_langid::LanguageIdentifier;

use crate::{options::Options, secrets::Secrets};

mod components;
mod diagrams;
mod error;
mod forecast_areas;
mod fs;
mod google_drive;
mod i18n;
mod observations;
mod options;
mod secrets;

#[derive(Clone)]
struct AppState {
    secrets: &'static Secrets,
    client: reqwest::Client,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    axum_reporting::setup_error_hooks()?;

    let options_init = Options::initialize().await;
    let options: &'static Options = options_init
        .result
        .map(|options| Box::leak(Box::new(options)))
        .map_err(|error| {
            options_init.logs.print();
            error
        })?;

    fs::create_dir_if_not_exists(&options.data_dir)
        .wrap_err_with(|| format!("Unable to create data directory {:?}", options.data_dir))
        .map_err(|error| {
            options_init.logs.print();
            error
        })?;

    let reporting_options: &'static axum_reporting::Options =
        Box::leak(Box::new(axum_reporting::Options {
            default_filter: "warn,avalanche_report=debug".to_owned(),
            page_title: "avalanche-report".to_owned(),
            data_dir: options.data_dir.clone(),
            log_rotation: Rotation::DAILY,
            log_file_name: "avalanche-report".to_owned(),
        }));

    let _reporting_guard = axum_reporting::setup_logging(reporting_options).map_err(|error| {
        options_init.logs.print();
        error
    })?;

    options_init.logs.present();

    let secrets = Box::leak(Box::new(
        Secrets::initialize(&options.secrets_dir)
            .await
            .wrap_err("Error while initializing secrets")?,
    ));

    i18n::load_languages().wrap_err("Error loading languages")?;

    let state = AppState {
        secrets,
        client: reqwest::Client::new(),
    };

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(index_handler))
        .route("/forecasts/:file_id", get(forecast_handler))
        .nest("/diagrams", diagrams::router())
        .nest("/observations", observations::router())
        .nest("/forecast-areas", forecast_areas::router())
        .route_service("/dist/*file", dist_handler.into_service())
        .route_service("/static/*file", static_handler.into_service())
        .with_state(state)
        .nest("/logs", axum_reporting::serve_logs(reporting_options))
        .fallback(not_found_handler)
        .layer(middleware::from_fn(i18n::middleware));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = &options.listen_address;
    tracing::info!("listening on {addr}");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

#[derive(Clone, Serialize)]
struct ForecastFileDetails {
    forecast: ForecastDetails,
    language: LanguageIdentifier,
}

#[derive(Serialize, PartialEq, Eq, Clone)]
struct ForecastDetails {
    area: String,
    time: DateTime<chrono_tz::Tz>,
    forecaster: String,
}

#[derive(Clone)]
struct ForecastFile {
    details: ForecastFileDetails,
    file: FileMetadata,
}

struct Forecast {
    details: ForecastDetails,
    files: Vec<ForecastFile>,
}

fn parse_forecast_details(file_name: &str) -> eyre::Result<ForecastFileDetails> {
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
    let time = NaiveDateTime::parse_from_str(&time_string, "%Y-%m-%dT%H:%M")
        .wrap_err_with(|| format!("Error parsing time {time_string:?}"))?;
    let time = match time.and_local_timezone(chrono_tz::Asia::Tbilisi) {
        chrono::LocalResult::Single(time) => Ok(time),
        unexpected => Err(eyre::eyre!(
            "Unable to convert into Tbilisi timezone: {unexpected:?}"
        )),
    }?;
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

fn format_language_name(language: &LanguageIdentifier) -> Option<String> {
    match language.language.as_str() {
        "en" => Some("English".to_owned()),
        "ka" => Some("ქართული".to_owned()),
        _ => None,
    }
}

async fn index_handler(
    Extension(loader): Extension<I18nLoader>,
    State(state): State<AppState>,
) -> axum::response::Result<impl IntoResponse> {
    Ok(index_handler_impl(&state.client, loader, state.secrets)
        .await
        .map_err(handle_eyre_error)?)
}

async fn index_handler_impl(
    client: &reqwest::Client,
    loader: I18nLoader,
    secrets: &Secrets,
) -> eyre::Result<Response> {
    let files = match &secrets.google_drive_api_key {
        Some(api_key) => {
            google_drive::list_files("1so1EaO5clMvBUecCszKlruxnf0XpbWgr", api_key, &client)
                .await
                .wrap_err("Error listing google drive files")?
        }
        None => {
            tracing::warn!("Unable to list files, no Google Drive api key secret is specified");
            Vec::new()
        }
    };
    let (mut forecasts, errors): (Vec<Forecast>, Vec<eyre::Error>) = files
        .into_iter()
        .filter(|file| file.mime_type == "application/pdf")
        .map(|file| {
            let filename = &file.name;
            let details = parse_forecast_details(filename).wrap_err_with(|| {
                    eyre::eyre!("Error parsing forecast details from file {filename:?}")
                })
                .suggestion("Name file according to the standard format.\n e.g. \"Gudauri_2023-01-24T17:00_LF.en.pdf\"")?;
            Ok(ForecastFile { details, file })
        })
        .fold((Vec::new(), Vec::new()), |mut acc, result| {
            match result {
                Ok(forecast_file) => {
                    if let Some(i) = acc.0.iter().position(|forecast| forecast.details == forecast_file.details.forecast) {
                        acc.0.get_mut(i).unwrap().files.push(forecast_file)
                    } else {
                        acc.0.push(Forecast { details: forecast_file.details.forecast.clone(), files: vec![forecast_file] });
                    }
                }
                Err(err) => acc.1.push(err),
            }
            acc
        });

    forecasts.sort_by(|a, b| b.details.time.cmp(&a.details.time));

    Ok(components::Base::builder()
        .i18n(loader.clone())
        .body(&|body: &mut Node| {
            let mut h1 = body.h1().attr(r#"class="text-3xl font-bold""#);
            h1.write_str("Gudauri Avalanche Forecasts")?;

            for (i, forecast) in forecasts.iter().enumerate() {
                let time = &forecast.details.time;
                if i == 0 {
                    body.h2()
                        .attr(r#"class="text-2xl font-bold""#)
                        .write_str(&format!("Current Forecast - {time}"))?;
                } else {
                    if i == 1 {
                        body.h2()
                            .attr(r#"class="text-2xl font-bold""#)
                            .write_str(&format!("Forecast Archive"))?;
                    }
                    body.h3()
                        .attr(r#"class="text-xl font-bold""#)
                        .write_str(&format!("{time}"))?;
                }


                let mut files = forecast.files.clone();
                files.sort_by(|a, b| a.details.language.cmp(&b.details.language));

                for forecast_file in &forecast.files {
                    let file = &forecast_file.file;
                    let file_id = &file.id;
                    let name = &file.name;
                    let text = format_language_name(&forecast_file.details.language)
                        .map(|language| format!("{language} (PDF)"))
                        .unwrap_or_else(|| file.name.clone());

                    body.a()
                        .attr(
                            r#"class="text-blue-600 hover:text-blue-800 visited:text-purple-600""#,
                        )
                        .attr(&format!(r#"href="/forecasts/{file_id}""#))
                        .attr(&format!(r#"download="{name}""#))
                        .write_str(&text)?;
                }
                body.br();
            }

            if !errors.is_empty() {
                body.h2()
                    .attr(r#"class="text-2xl font-bold text-rose-600""#)
                    .write_str("Errors Reading Forecast Files")?;
                for (i, error) in errors.iter().enumerate() {
                    body.h3()
                        .attr(r#"class="text-xl font-bold text-rose-600""#)
                        .write_str(&format!("Error {}", i + 1))?;
                    let html_error = ansi_to_html::convert_escaped(&format!("{error:?}").trim())?
                        .replace('\n', "<br>");

                    body.p().write_str(&html_error)?;
                }
            }

            Ok(())
        })
        .build()
        .into_response())
}

async fn forecast_handler(
    Path(file_id): Path<String>,
    State(state): State<AppState>,
) -> axum::response::Result<impl IntoResponse> {
    let google_drive_api_key = state.secrets.google_drive_api_key.as_ref().ok_or_else(|| {
        tracing::error!("Unable to fetch file, Google Drive API Key not specified");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(
        forecast_handler_impl(&file_id, google_drive_api_key, &state.client)
            .await
            .map_err(handle_eyre_error)?,
    )
}

async fn forecast_handler_impl(
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

async fn dist_handler(uri: Uri, Extension(loader): Extension<I18nLoader>) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.starts_with("dist/") {
        path = path.replace("dist/", "");
    }

    DistFile::get(path, loader)
}

async fn static_handler(uri: Uri, Extension(loader): Extension<I18nLoader>) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.starts_with("static/") {
        path = path.replace("static/", "");
    }

    StaticFile::get(path, loader)
}

/// Create a 404 not found response
async fn not_found_handler(
    Extension(loader): Extension<I18nLoader>,
) -> axum::response::Result<impl IntoResponse> {
    not_found(loader)
}

fn not_found(loader: I18nLoader) -> axum::response::Result<impl IntoResponse> {
    Ok((
        StatusCode::NOT_FOUND,
        components::Base::builder()
            .i18n(loader)
            .body(&|body: &mut Node| {
                body.h1().write_str("404")?;
                body.p().write_str("Not Found")?;
                Ok(())
            })
            .build()
            .into_response(),
    ))
}

#[derive(RustEmbed)]
#[folder = "dist"]
struct DistDir;
type DistFile<T> = EmbeddedFile<DistDir, T>;

#[derive(RustEmbed)]
#[folder = "static"]
struct StaticDir;
type StaticFile<T> = EmbeddedFile<StaticDir, T>;

pub struct EmbeddedFile<E, T> {
    pub path: T,
    loader: I18nLoader,
    embed: PhantomData<E>,
}

impl<E, T> EmbeddedFile<E, T> {
    pub fn get(path: T, loader: I18nLoader) -> Self {
        Self {
            path,
            loader,
            embed: PhantomData,
        }
    }
}

impl<E, T> IntoResponse for EmbeddedFile<E, T>
where
    E: RustEmbed,
    T: AsRef<str>,
{
    fn into_response(self) -> Response {
        let path: &str = self.path.as_ref();
        match E::get(path) {
            Some(content) => {
                let body = boxed(Full::from(content.data));
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                Response::builder()
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .body(body)
                    .unwrap()
            }
            None => not_found(self.loader).into_response(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::parse_forecast_details;

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
