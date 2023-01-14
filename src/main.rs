use std::net::SocketAddr;

use axum::{
    body::{boxed, Full},
    http::{header, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router, handler::HandlerWithoutStateExt,
};
use html_builder::Html5;
use rust_embed::RustEmbed;
use std::fmt::Write;

#[tokio::main]
async fn main() {
    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(index))
        .route("/clicked", post(clicked))
        .route_service("/dist/*file", dist_handler.into_service())
        .fallback_service(get(not_found));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("listening on {addr}");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn clicked() -> Html<&'static str> {
    Html("Clicked")
}

async fn index() -> Html<String> {
    index_impl().unwrap()
}

fn index_impl() -> Result<Html<String>, std::fmt::Error> {
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
    writeln!(h1, "Hello World!")?;

    body.write_str(r##"
    <button id="button" hx-post="/clicked"
        hx-trigger="click"
            hx-target="#button"
        hx-swap="outerHTML"
    >
        Click Me!
    </button>
    "##).unwrap();

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
