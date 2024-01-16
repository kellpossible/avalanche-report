use axum::{
    extract::Query,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    Extension,
};
use axum_extra::extract::CookieJar;
use base64::Engine;
use eyre::{Context, ContextCompat};
use http::{header::SET_COOKIE, HeaderMap, HeaderValue, Request, StatusCode};
use serde::{Deserialize, Serialize};

use crate::error::{map_eyre_error, map_std_error};

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(default)]
pub struct UserPreferences {
    pub lang: Option<unic_langid::LanguageIdentifier>,
}

impl UserPreferences {
    /// Merge right into left, skipping any fields that are `None` on right.
    fn merge(mut left: Self, right: Self) -> Self {
        if right.lang.is_some() {
            left.lang = right.lang;
        }

        left
    }
}

const COOKIE_NAME: &str = "preferences";
/// The Max-Age property for the cookie (in seconds).
const COOKIE_MAX_AGE_SECONDS: u64 = 365 * 24 * 60 * 60;

/// Handler for setting user preferences using a query, and redirecting to the referrer URL
/// provided in the request. This merges with what has currently been set.
pub async fn handler(
    Query(set_preferences): Query<UserPreferences>,
    Extension(current_preferences): Extension<UserPreferences>,
    headers: HeaderMap,
) -> axum::response::Result<impl IntoResponse> {
    let referer_str = headers
        .get("Referer")
        .wrap_err("No referer headers")
        .map_err(map_eyre_error)?
        .to_str()
        .wrap_err("Referer is not a valid string")
        .map_err(map_eyre_error)?;
    let mut response = Redirect::to(referer_str).into_response();

    let preferences = UserPreferences::merge(current_preferences, set_preferences);
    let preferences_data = serde_urlencoded::to_string(preferences)
        .context("Error serializing preferences")
        .map_err(map_eyre_error)?;
    let value = HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={preferences_data}; Max-Age={COOKIE_MAX_AGE_SECONDS}"
    ))
    .map_err(map_std_error)?;
    response.headers_mut().insert(SET_COOKIE, value);
    Ok(response)
}

/// Middleware for extracting user preferences from cookie that was set using  [`set_handler`].
pub async fn middleware<B>(mut request: Request<B>, next: Next<B>) -> Response {
    let cookies = CookieJar::from_headers(request.headers());
    let preferences: UserPreferences =
        match Option::transpose(cookies.get(COOKIE_NAME).map(|cookie| {
            serde_urlencoded::from_str(cookie.value()).context("Error deserializing preferences")
        })) {
            Ok(preferences) => preferences.unwrap_or_default(),
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Unable to parse preferences cookie: {error}"),
                )
                    .into_response()
            }
        };

    request.extensions_mut().insert(preferences);
    next.run(request).await
}
