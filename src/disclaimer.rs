//! Middleware for a disclaimer message which needs to appear on any of our forecast pages.

use crate::{
    error::{map_eyre_error, map_std_error},
    isbot::IsBot,
    templates::{render, TemplatesWithContext},
};
use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use eyre::{Context, ContextCompat};
use http::{header::SET_COOKIE, HeaderMap, HeaderValue};

const DISCLAIMER_COOKIE_NAME: &str = "disclaimer";
/// TODO: if this version is updated we need new logic to require the current version of the
/// cookie, otherwise the user should re-accept the updated disclaimer. There should be an
/// alternate message explaining that the disclaimer has been updated if the user has already
/// accepted the pevious version.
const DISCLAIMER_VERSION: u32 = 1;

/// The Max-Age property for the cookie (in seconds).
const DISCLAIMER_COOKIE_MAX_AGE_SECONDS: u64 = 365 * 24 * 60 * 60;

/// Handler to accept the disclaimer by setting a cookie [`DISCLAIMER_COOKIE_NAME`].
pub async fn handler(headers: HeaderMap) -> axum::response::Result<impl IntoResponse> {
    let referer_str = headers
        .get("Referer")
        .wrap_err("No referer headers")
        .map_err(map_eyre_error)?
        .to_str()
        .wrap_err("Referer is not a valid string")
        .map_err(map_eyre_error)?;

    let mut response = Redirect::to(referer_str).into_response();
    let value = HeaderValue::from_str(&format!("{DISCLAIMER_COOKIE_NAME}=v{DISCLAIMER_VERSION}; Max-Age={DISCLAIMER_COOKIE_MAX_AGE_SECONDS}"))
        .map_err(map_std_error)?;
    response.headers_mut().insert(SET_COOKIE, value);
    Ok(response)
}

pub async fn middleware(request: Request, next: Next) -> Response {
    let is_bot = request
        .extensions()
        .get::<IsBot>()
        .expect("Expected extension IsBot to be available")
        .is_bot();
    let cookies = CookieJar::from_headers(request.headers());
    if is_bot || cookies.get(DISCLAIMER_COOKIE_NAME).is_some() {
        return next.run(request).await;
    }

    let templates: &TemplatesWithContext = request.extensions().get().unwrap();
    render(&templates.environment, "disclaimer.html", &())
        .unwrap()
        .into_response()
}
