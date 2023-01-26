use axum::{
    http::{header::CONTENT_TYPE, HeaderValue},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

pub fn router() -> Router {
    Router::new().route("/gudauri/area.geojson", get(gudauri_area_handler))
}

pub async fn gudauri_area_handler() -> impl IntoResponse {
    Response::builder()
        .header(
            CONTENT_TYPE,
            "application/geo+json".parse::<HeaderValue>().unwrap(),
        )
        .body(gudauri_area())
        .expect("Unable to build response")
}

pub fn gudauri_area() -> String {
    include_str!("./gudauri.geojson").to_string()
}
