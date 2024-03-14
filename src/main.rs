use axum::{
    handler::HandlerWithoutStateExt,
    http::{header, StatusCode, Uri},
    middleware,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Extension, Router,
};
use bytes::Bytes;
use error::map_std_error;
use eyre::Context;
use rust_embed::RustEmbed;
use std::marker::PhantomData;
use templates::TemplatesWithContext;
use tower_http::trace::TraceLayer;
use tracing_appender::rolling::Rotation;

use crate::{
    analytics::CompactionConfig,
    current_weather::{
        CurrentWeatherCacheService, CurrentWeatherCacheServiceConfig, CurrentWeatherService,
    },
    database::backup,
    options::Options,
    state::AppState,
    templates::Templates,
};

mod admin;
mod analytics;
mod auth;
mod cache_control;
mod current_weather;
mod database;
mod diagrams;
mod disclaimer;
mod error;
mod forecast_areas;
mod forecasts;
mod fs;
mod google_drive;
mod i18n;
mod index;
mod isbot;
mod observations;
mod options;
mod serde;
mod state;
mod templates;
mod types;
mod user_preferences;
mod utilities;
mod weather;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    axum_reporting::setup_error_hooks()?;

    let options: &'static Options = Box::leak(Box::new(Options::initialize().await?));

    fs::create_dir_if_not_exists(&options.data_dir)
        .wrap_err_with(|| format!("Unable to create data directory {:?}", options.data_dir))?;

    let reporting_options: &'static axum_reporting::Options =
        Box::leak(Box::new(axum_reporting::Options {
            default_filter: "warn,avalanche_report=info".to_owned(),
            page_title: "avalanche-report".to_owned(),
            data_dir: options.data_dir.clone(),
            log_rotation: Rotation::DAILY,
            log_file_name: "avalanche-report".to_owned(),
        }));

    let _reporting_guard = axum_reporting::initialize(reporting_options)?;

    let client = reqwest::Client::new();

    let (i18n, _watcher_guard) =
        i18n::initialize(&options.i18n).wrap_err("Error initializing i18n")?;
    crate::i18n::load_available_languages(&i18n, &options.default_language_order)
        .wrap_err("Error loading languages")?;

    let templates = Templates::initialize(&options.templates)?;

    let database = database::initialize(&options.data_dir)
        .await
        .wrap_err("Error initializing database")?;

    if let Some(backup) = &options.backup {
        backup::spawn_backup_task(backup::Config {
            client: client.clone(),
            backup,
            aws_secret_access_key: &backup.aws_secret_access_key,
            database: database.clone(),
        });
    }

    analytics::spawn_compaction_task(CompactionConfig {
        schedule: options.analytics.compaction_schedule.clone(),
        database: database.clone(),
    });

    let (analytics_sx, analytics_rx) = analytics::channel();
    let database_analytics = database.clone();
    tokio::spawn(async move {
        analytics::process_analytics(
            database_analytics,
            analytics_rx,
            options.analytics.event_batch_rate,
        )
        .await
    });

    let current_weather = std::sync::Arc::new(CurrentWeatherService::new(
        database.clone(),
        options.weather_stations.clone(),
    ));

    CurrentWeatherCacheService::new(CurrentWeatherCacheServiceConfig {
        interval: std::time::Duration::from_secs(60),
        each_station_interval: std::time::Duration::from_secs(1),
        weather_stations: &options.weather_stations,
        client: client.clone(),
        database: database.clone(),
    })
    .spawn();

    let state = AppState {
        options,
        client: client.clone(),
        i18n,
        templates,
        database,
        analytics_sx,
        current_weather,
    };

    // build our application with a route
    let app = Router::new()
        // All these pages are dynamic and should have the Cache-Control: no-store header set
        // using the cache_control::no_store_middleware to help prevent browsers from caching them
        // and preventing updates during refresh.
        .nest(
            "/",
            Router::new()
                // Using a GET request because this supports a redirect.
                .route(
                    "/user-preferences-redirect",
                    get(user_preferences::query_set_redirect_handler),
                )
                .route("/disclaimer", post(disclaimer::handler))
                .route("/weather", get(weather::handler))
                // These routes expose public forecast information and thus have the disclaimer middleware
                // applied to them.
                .nest(
                    "/",
                    Router::new()
                        .route("/", get(index::handler))
                        .route("/forecasts/:file_name", get(forecasts::handler))
                        .nest("/observations", observations::router())
                        .layer(middleware::from_fn(disclaimer::middleware)),
                )
                .nest(
                    "/admin",
                    admin::router(reporting_options, &options.admin_password_hash),
                )
                .layer(middleware::from_fn(cache_control::no_store_middleware)),
        )
        .nest("/current-weather", current_weather::router())
        .nest("/diagrams", diagrams::router())
        .nest("/forecast-areas", forecast_areas::router())
        .route_service("/dist/*file", dist_handler.into_service())
        .route_service("/static/*file", static_handler.into_service())
        .fallback(not_found_handler)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            templates::middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            i18n::middleware,
        ))
        .layer(middleware::from_fn(user_preferences::middleware))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            analytics::middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            database::middleware,
        ))
        .layer(middleware::from_fn(isbot::middleware))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let url = &options.base_url();
    tracing::info!("listening on {url}");
    let listener = tokio::net::TcpListener::bind(&options.listen_address).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn dist_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.starts_with("dist/") {
        path = path.replace("dist/", "");
    }

    DistFile::get(path)
}

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.starts_with("static/") {
        path = path.replace("static/", "");
    }

    StaticFile::get(path)
}

/// Create a 404 not found response
async fn not_found_handler(
    Extension(templates): Extension<TemplatesWithContext>,
) -> axum::response::Result<impl IntoResponse> {
    not_found(templates)
}

fn not_found(templates: TemplatesWithContext) -> axum::response::Result<impl IntoResponse> {
    let template = templates
        .environment
        .get_template("404.html")
        .map_err(map_std_error)?;
    let body = Html(template.render(()).map_err(map_std_error)?);
    Ok((StatusCode::NOT_FOUND, body))
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
    embed: PhantomData<E>,
}

impl<E, T> EmbeddedFile<E, T> {
    pub fn get(path: T) -> Self {
        Self {
            path,
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
                let bytes = Bytes::from(content.data.to_vec());
                let body = axum::body::Body::from(bytes);
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                Response::builder()
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .body(body)
                    .unwrap()
            }
            None => StatusCode::NOT_FOUND.into_response(),
        }
    }
}
