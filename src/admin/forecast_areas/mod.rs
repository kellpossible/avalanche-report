use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;

mod create;
mod index;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index::handler))
        .route("/create", get(create::get_handler))
        .route("/create", post(create::post_handler))
}
