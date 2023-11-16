use axum::{routing::get, Router};
use once_cell::sync::Lazy;
use usvg_text_layout::fontdb;

pub mod aspect_elevation;
mod elevation_hazard;
pub mod probability;
pub mod size;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/elevation_hazard.svg", get(elevation_hazard::svg_handler))
        .route("/elevation_hazard.png", get(elevation_hazard::png_handler))
        .route("/aspect_elevation.svg", get(aspect_elevation::svg_handler))
        .route("/aspect_elevation.png", get(aspect_elevation::png_handler))
        .route("/size.svg", get(size::svg_handler))
        .route("/probability.svg", get(probability::svg_handler))
}

const FONT_DATA: &[u8] = include_bytes!("./fonts/noto/NotoSans-RegularWithGeorgian.ttf");
static FONT_DB: Lazy<fontdb::Database> = Lazy::new(|| {
    let mut db = fontdb::Database::new();
    db.load_font_data(FONT_DATA.to_vec());
    db.set_sans_serif_family("Noto Sans");
    db
});
