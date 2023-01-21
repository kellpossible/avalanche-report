use axum::{routing::get, Router};

mod elevation_hazard;

pub fn router() -> Router {
    Router::new()
        .route("/elevation_hazard.svg", get(elevation_hazard::svg_handler))
        .route("/elevation_hazard.png", get(elevation_hazard::png_handler))
}
