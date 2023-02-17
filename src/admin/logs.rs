use axum::Router;

pub fn router<S>(reporting_options: &'static axum_reporting::Options) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().nest("/", axum_reporting::serve_logs(reporting_options))
}
