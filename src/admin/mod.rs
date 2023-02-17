use axum::{routing::get, Router};
use secrecy::SecretString;
use tower_http::auth::RequireAuthorizationLayer;

use crate::{auth::MyBasicAuth, state::AppState};

mod analytics;
mod logs;

pub fn router(
    reporting_options: &'static axum_reporting::Options,
    admin_password_hash: &'static SecretString,
) -> Router<AppState> {
    Router::new()
        .route("/analytics", get(analytics::handler))
        .nest("/logs", logs::router(reporting_options))
        .layer(RequireAuthorizationLayer::custom(MyBasicAuth {
            admin_password_hash,
        }))
}
