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
use html_builder::Html5;
use i18n::I18nLoader;
use i18n_embed_fl::fl;
use rust_embed::RustEmbed;
use std::fmt::Write;
use tracing_appender::rolling::Rotation;

use crate::options::Options;

mod diagrams;
mod fs;
mod i18n;
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
        .route("/", get(index))
        .nest("/diagrams", diagrams::router())
        .nest("/logs/", axum_reporting::serve_logs(reporting_options))
        .route("/clicked", post(clicked))
        .route_layer(middleware::from_fn(i18n::middleware))
        .route_service("/dist/*file", dist_handler.into_service())
        .fallback_service(get(not_found));

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

async fn index(Extension(loader): Extension<I18nLoader>) -> Html<String> {
    index_impl(loader).unwrap()
}

fn index_impl(loader: I18nLoader) -> Result<Html<String>, std::fmt::Error> {
    let mut buf = html_builder::Buffer::new();
    buf.doctype();
    let mut html = buf.html();
    let mut head = html.head();
    head.write_str(
        r#"<meta charset="UTF-8">
          <meta name="viewport" content="width=device-width, initial-scale=1.0">
          <link href="/dist/output.css" rel="stylesheet">
          "#,
    )?;
    let mut body = html.body();
    let mut h1 = body.h1().attr(r#"class="text-3xl font-bold underline""#);
    h1.write_str(&fl!(loader, "hello-world"))?;

    let mut button = body
        .button()
        .attr(r#"id="button""#)
        .attr(r#"hx-post="/clicked""#)
        .attr(r##"hx-target="#button""##)
        .attr(r#"hx-swap="outerHTML""#);
    button.write_str(&fl!(loader, "button-click-me"))?;

    body.write_str(r#"<script src="/dist/main.js"></script>"#)?;

    Ok(Html(buf.finish()))
}

// We use a wildcard matcher ("/dist/*file") to match against everything
// within our defined assets directory. This is the directory on our Asset
// struct below, where folder = "examples/public/".
async fn dist_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();

    if path.starts_with("dist/") {
        path = path.replace("dist/", "");
    }

    StaticFile(path)
}

// Finally, we use a fallback route for anything that didn't match.
async fn not_found() -> Html<&'static str> {
    Html("<h1>404</h1><p>Not Found</p>")
}

#[derive(RustEmbed)]
#[folder = "dist"]
struct DistDir;

pub struct StaticFile<T>(pub T);

impl<T> IntoResponse for StaticFile<T>
where
    T: Into<String>,
{
    fn into_response(self) -> Response {
        let path = self.0.into();

        match DistDir::get(path.as_str()) {
            Some(content) => {
                let body = boxed(Full::from(content.data));
                let mime = mime_guess::from_path(path).first_or_octet_stream();
                Response::builder()
                    .header(header::CONTENT_TYPE, mime.as_ref())
                    .body(body)
                    .unwrap()
            }
            None => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(boxed(Full::from("404")))
                .unwrap(),
        }
    }
}
