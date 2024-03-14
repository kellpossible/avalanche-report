use axum::{routing::get, Router};
use secrecy::SecretString;
use tower_http::auth::AsyncRequireAuthorizationLayer;

use crate::{auth::MyBasicAuth, state::AppState, templates};

mod analytics;
mod forecast_areas;
mod logs;

pub struct Config {
    pub reporting: &'static axum_reporting::Options,
    pub admin_password_hash: &'static SecretString,
}

pub fn router(
    config: Config
) -> Router<AppState> {
    Router::new()
        .route("/", get(templates::create_handler("admin/index.html")))
        .nest("/analytics", analytics::router())
        .nest("/logs", logs::router(config.reporting))
        .nest("/forecast-areas", forecast_areas::router())
        .layer(AsyncRequireAuthorizationLayer::new(MyBasicAuth::new(
            config.admin_password_hash,
        )))
}
