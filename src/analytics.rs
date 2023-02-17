use axum::{middleware::Next, response::Response};
use http::Request;

/// Middleware for performing analytics on incoming requests.
pub async fn middleware<B>(request: Request<B>, next: Next<B>) -> Response {
    // TODO: write to the database
    let response = next.run(request).await;
    response
}
