use axum::{routing::get, Router};
use secrecy::SecretString;
use tower_http::auth::AsyncRequireAuthorizationLayer;

use crate::{auth::MyBasicAuth, state::AppState, templates};

mod analytics;
mod forecast_areas;
mod forecast_files;
mod logs;
mod sqlite;

pub struct Config {
    pub reporting: &'static axum_reporting::Options,
    // Whether to enable the SQLite admin interface.
    pub sqlite_enabled: bool,
    pub admin_password_hash: &'static SecretString,
}

pub fn router(config: Config) -> Router<AppState> {
    let mut router = Router::new()
        .route("/", get(templates::create_handler("admin/index.html")))
        .nest("/analytics", analytics::router())
        .nest("/logs", logs::router(config.reporting))
        .nest("/forecast-areas", forecast_areas::router())
        .nest("/forecast-files", forecast_files::router());

    if config.sqlite_enabled {
        router = router.nest("/sqlite", sqlite::router());
    }

    router.layer(AsyncRequireAuthorizationLayer::new(MyBasicAuth::new(
        config.admin_password_hash,
    )))
}
