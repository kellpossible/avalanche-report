use axum::{routing::get, Router};

use crate::state::AppState;

mod create;
mod index;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index::handler))
        .nest("/create", create::router())
}
