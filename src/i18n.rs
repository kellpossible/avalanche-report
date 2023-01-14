use axum::{
    http::{HeaderMap, Request},
    middleware::Next,
    response::Response,
};

async fn i18n<B>(headers: HeaderMap, request: Request<B>, next: Next<B>) -> Response {
    //TODO
}
