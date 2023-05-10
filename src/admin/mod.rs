use axum::Router;
use secrecy::SecretString;
use tower_http::auth::AsyncRequireAuthorizationLayer;

use crate::{auth::MyBasicAuth, state::AppState};

mod analytics;
mod logs;

pub fn router(
    reporting_options: &'static axum_reporting::Options,
    admin_password_hash: &'static SecretString,
) -> Router<AppState> {
    Router::new()
        .nest("/analytics", analytics::router())
        .nest("/logs", logs::router(reporting_options))
        .layer(AsyncRequireAuthorizationLayer::new(MyBasicAuth::new(
            admin_password_hash,
        )))
}
