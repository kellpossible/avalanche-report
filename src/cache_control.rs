use axum::{extract::Request, middleware::Next, response::Response};
use headers::{CacheControl, HeaderMapExt};

/// Middleware to set the [`CacheControl`] header on all reponses to `no-store` to prevent browsers
/// from caching dynamic pages and causing unexpected lag in updates.
pub async fn no_store_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .typed_insert(CacheControl::new().with_no_store());
    response
}
