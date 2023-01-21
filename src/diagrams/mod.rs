use axum::{routing::get, Router};

mod aspect_elevation;
mod elevation_hazard;

pub fn router() -> Router {
    Router::new()
        .route("/elevation_hazard.svg", get(elevation_hazard::svg_handler))
        .route("/elevation_hazard.png", get(elevation_hazard::png_handler))
        .route("/aspect_elevation.svg", get(aspect_elevation::svg_handler))
        .route("/aspect_elevation.png", get(aspect_elevation::png_handler))
}
