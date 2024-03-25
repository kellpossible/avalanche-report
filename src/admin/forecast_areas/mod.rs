use axum::{routing::get, Router};

use crate::state::AppState;

mod create;
mod edit;
mod index;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index::handler))
        .nest("/create", create::router())
        .nest("/:forecast_area_id/edit", edit::router())
}
