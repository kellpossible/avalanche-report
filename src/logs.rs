use axum::Router;
use secrecy::SecretString;
use tower_http::auth::RequireAuthorizationLayer;

use crate::auth::MyBasicAuth;

pub fn router<S>(
    reporting_options: &'static axum_reporting::Options,
    admin_password_hash: &'static SecretString,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .nest("/", axum_reporting::serve_logs(reporting_options))
        .layer(RequireAuthorizationLayer::custom(MyBasicAuth {
            admin_password_hash,
        }))
}
