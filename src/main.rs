use axum::{
    body::{boxed, Full},
    handler::HandlerWithoutStateExt,
    http::{header, StatusCode, Uri},
    middleware,
    response::{Html, IntoResponse, Response},
    routing::get,
    Extension, Router,
};
use error::map_std_error;
use eyre::Context;
use rust_embed::RustEmbed;
use std::marker::PhantomData;
use templates::TemplatesWithContext;
use tower_http::trace::TraceLayer;
use tracing_appender::rolling::Rotation;

use crate::{options::Options, secrets::Secrets, state::AppState, templates::Templates};

mod admin;
mod analytics;
mod auth;
mod database;
mod diagrams;
mod env;
mod error;
mod forecast_areas;
mod forecasts;
mod fs;
mod google_drive;
mod i18n;
mod index;
mod observations;
mod options;
mod secrets;
mod serde;
mod state;
mod templates;
mod types;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    axum_reporting::setup_error_hooks()?;

    env::initialize()?;

    let options: &'static Options = Box::leak(Box::new(Options::initialize().await?));

    fs::create_dir_if_not_exists(&options.data_dir)
        .wrap_err_with(|| format!("Unable to create data directory {:?}", options.data_dir))?;

    let reporting_options: &'static axum_reporting::Options =
        Box::leak(Box::new(axum_reporting::Options {
            default_filter: "warn,avalanche_report=debug".to_owned(),
            page_title: "avalanche-report".to_owned(),
            data_dir: options.data_dir.clone(),
            log_rotation: Rotation::DAILY,
            log_file_name: "avalanche-report".to_owned(),
        }));

    let _reporting_guard = axum_reporting::initialize(reporting_options)?;

    let secrets = Box::leak(Box::new(Secrets::initialize()?));

    let i18n = i18n::initialize();
    crate::i18n::load_languages(&i18n).wrap_err("Error loading languages")?;

    let templates = Templates::initialize()?;

    let database = database::initialize(&options.data_dir)
        .await
        .wrap_err("Error initializing database")?;

    let (analytics_sx, analytics_rx) = analytics::channel();
    let database_analytics = database.clone();
    tokio::spawn(async move {
        analytics::process_analytics(
            database_analytics,
            analytics_rx,
            options.analytics_batch_rate,
        )
        .await
    });

    let state = AppState {
        options,
        secrets,
        client: reqwest::Client::new(),
        i18n,
        templates,
        database,
        analytics_sx,
    };

    // build our application with a route
    let app = Router::new()
        .route("/", get(index::handler))
        .route("/i18n", get(i18n::handler))
        .route("/forecasts/:file_name", get(forecasts::handler))
        .nest("/diagrams", diagrams::router())
        .nest("/observations", observations::router())
        .nest("/forecast-areas", forecast_areas::router())
        .route_service("/dist/*file", dist_handler.into_service())
        .route_service("/static/*file", static_handler.into_service())
        .nest(
            "/admin",
            admin::router(reporting_options, &secrets.admin_password_hash),
        )
        .fallback(not_found_handler)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            templates::middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            i18n::middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            analytics::middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            database::middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = &options.listen_address;
    tracing::info!("listening on http://{addr}");
    axum::Server::bind(addr)
        .serve(app.into_make_service())
        .await?;

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
                let body = boxed(Full::from(content.data));
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
