//! Middleware for a disclaimer message which needs to appear on any of our forecast pages.

use crate::{
    error::{map_eyre_error, map_std_error},
    serde::string,
    templates::{render, TemplatesWithContext},
    types::Uri,
};
use axum::{
    body::Body,
    extract::State,
    middleware::Next,
    response::{Html, IntoResponse, Redirect, Response},
    Extension,
};
use axum_extra::extract::CookieJar;
use eyre::{Context, ContextCompat};
use http::{header::SET_COOKIE, HeaderMap, HeaderValue, Request};
use serde::Deserialize;

use crate::state::AppState;

const DISCLAIMER_COOKIE_NAME: &str = "disclaimer";

/// Handler to accept the disclaimer by setting a cookie [`DISCLAIMER_COOKIE_NAME`].
pub async fn handler(headers: HeaderMap) -> axum::response::Result<impl IntoResponse> {
    let referer_str = headers
        .get("Referer")
        .wrap_err("No referer headers")
        .map_err(map_eyre_error)?
        .to_str()
        .wrap_err("Referer is not a valid string")
        .map_err(map_eyre_error)?;

    let mut response = Redirect::to("/").into_response();
    let value = HeaderValue::from_str(&format!("{DISCLAIMER_COOKIE_NAME}=accepted"))
        .map_err(map_std_error)?;
    response.headers_mut().insert(SET_COOKIE, value);
    Ok(response)
}

pub async fn middleware<B>(request: Request<B>, next: Next<B>) -> Response {
    let cookies = CookieJar::from_headers(request.headers());
    if cookies.get(DISCLAIMER_COOKIE_NAME).is_some()
        || request.uri() == &"/disclaimer".parse::<http::Uri>().unwrap()
    {
        return next.run(request).await;
    }

    let templates: &TemplatesWithContext = request.extensions().get().unwrap();
    render(&templates.environment, "disclaimer.html", &())
        .unwrap()
        .into_response()
}
