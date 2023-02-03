use axum::{
    body::{boxed, Full},
    handler::HandlerWithoutStateExt,
    http::{header, StatusCode, Uri},
    middleware,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Extension, Router,
};
use eyre::Context;
use html_builder::{Html5, Node};
use i18n::I18nLoader;
use i18n_embed_fl::fl;
use rust_embed::RustEmbed;
use std::{fmt::Write, marker::PhantomData};
use tracing_appender::rolling::Rotation;

use crate::options::Options;

mod components;
mod diagrams;
mod error;
mod forecast_areas;
mod fs;
mod i18n;
mod observations;
mod options;

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

    i18n::load_languages().wrap_err("Error loading languages")?;

    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(index_handler))
        .route("/clicked", post(clicked))
        .nest("/diagrams", diagrams::router())
        .nest("/logs", axum_reporting::serve_logs(reporting_options))
        .nest("/observations", observations::router())
        .nest("/forecast-areas", forecast_areas::router())
        .route_service("/dist/*file", dist_handler.into_service())
        .route_service("/static/*file", static_handler.into_service())
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

async fn clicked(Extension(loader): Extension<I18nLoader>) -> Html<String> {
    Html(fl!(loader, "button-clicked"))
}

async fn index_handler(Extension(loader): Extension<I18nLoader>) -> impl IntoResponse {
    components::Base::builder()
        .i18n(loader.clone())
        .body(&|body: &mut Node| {
            let mut h1 = body.h1().attr(r#"class="text-3xl font-bold underline""#);
            h1.write_str(&fl!(loader, "hello-world"))?;

            let mut button = body
                .button()
                .attr(r#"id="button""#)
                .attr(r#"hx-post="/clicked""#)
                .attr(r##"hx-target="#button""##)
                .attr(r#"hx-swap="outerHTML""#);
            button.write_str(&fl!(loader, "button-click-me"))?;
            Ok(())
        })
        .build()
        .into_response()
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
