use async_trait::async_trait;
use axum::{extract::FromRequestParts, middleware::Next, response::Response};
use http::{header::USER_AGENT, request::Parts, HeaderMap, Request, StatusCode};

pub struct IsBot(bool);

impl IsBot {
    pub fn is_bot(&self) -> bool {
        self.0
    }
}

static BOTS: once_cell::sync::Lazy<isbot::Bots> =
    once_cell::sync::Lazy::new(|| isbot::Bots::default());

pub fn is_bot(headers: &HeaderMap) -> bool {
    headers
        .get(USER_AGENT)
        .and_then(|user_agent| user_agent.to_str().ok())
        .map(|user_agent| BOTS.is_bot(user_agent))
        .unwrap_or(false)
}

/// Middleware to detect whether the request is from a bot based on the [`USER_AGENT`] header.
pub async fn middleware<B>(
    IsBot(is_bot): IsBot,
    mut request: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    request.extensions_mut().insert(is_bot);
    Ok(next.run(request).await)
}

#[async_trait]
impl<S> FromRequestParts<S> for IsBot {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(IsBot(is_bot(&parts.headers)))
    }
}
