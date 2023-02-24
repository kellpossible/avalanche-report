use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, Request},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::CookieJar;
use http::{header::SET_COOKIE, StatusCode, Uri};
use i18n_embed::{
    fluent::{fluent_language_loader, FluentLanguageLoader, NegotiationStrategy},
    LanguageLoader,
};
use rust_embed::RustEmbed;
use serde::Deserialize;
use std::{str::FromStr, sync::Arc};

use crate::{error::map_std_error, serde::string, state::AppState};

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub struct RequestedLanguages(pub Vec<unic_langid::LanguageIdentifier>);

impl std::fmt::Display for RequestedLanguages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", display_languages(&self.0))
    }
}

pub fn display_languages(languages: &[unic_langid::LanguageIdentifier]) -> String {
    let languages: String = languages
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<String>>()
        .join(", ");
    format!("[{languages}]")
}

fn parse_accept_language(accept_language: &HeaderValue) -> RequestedLanguages {
    RequestedLanguages(
        accept_language
            .to_str()
            .unwrap_or("")
            .split(',')
            .into_iter()
            .filter_map(|lang| lang.trim().parse::<unic_langid::LanguageIdentifier>().ok())
            .collect(),
    )
}

pub type I18nLoader = Arc<FluentLanguageLoader>;

// impl FromRef<AppState> for I18nLoader {
//     fn from_ref(state: &AppState) -> Self {
//         state.i18n
//     }
// }

pub fn initialize() -> I18nLoader {
    Arc::new(fluent_language_loader!())
}

pub fn load_languages(loader: &I18nLoader) -> eyre::Result<()> {
    let languages: String = display_languages(&loader.available_languages(&Localizations)?);
    loader.load_available_languages(&Localizations)?;
    tracing::info!("Localizations loaded. Available languages: {languages}");
    Ok(())
}

#[derive(Deserialize)]
pub struct Query {
    lang: unic_langid::LanguageIdentifier,
    #[serde(with = "string")]
    uri: Uri,
}

/// Handler to select a language by setting the `lang` cookie.
pub async fn handler(
    axum::extract::Query(query): axum::extract::Query<Query>,
) -> axum::response::Result<impl IntoResponse> {
    let mut response = Redirect::to(&query.uri.to_string()).into_response();
    let lang = query.lang;
    let value = HeaderValue::from_str(&format!("lang={lang}")).map_err(map_std_error)?;
    response.headers_mut().insert(SET_COOKIE, value);
    Ok(response)
}

pub async fn middleware<B>(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut request: Request<B>,
    next: Next<B>,
) -> Response {
    let cookies = CookieJar::from_headers(request.headers());
    let cookie_lang = match Option::transpose(
        cookies
            .iter()
            .find(|cookie| cookie.name() == "lang")
            .map(|cookie| unic_langid::LanguageIdentifier::from_str(cookie.value())),
    ) {
        Ok(cookie_lang) => cookie_lang,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Unable to parse lang cookie: {error}"),
            )
                .into_response()
        }
    };

    let accept_language = headers.get("Accept-Language").map(parse_accept_language);
    let requested_languages = cookie_lang
        .map(|lang| {
            let mut requested_languages = RequestedLanguages(vec![lang]);
            if let Some(accept_language) = &accept_language {
                requested_languages
                    .0
                    .extend(accept_language.0.iter().cloned())
            }
            requested_languages
        })
        .or_else(|| accept_language);

    let loader: I18nLoader = if let Some(requested_languages) = requested_languages {
        let loader = Arc::new(
            state
                .i18n
                .select_languages_negotiate(&requested_languages.0, NegotiationStrategy::Filtering),
        );
        request.extensions_mut().insert(requested_languages);
        loader
    } else {
        state.i18n.clone()
    };

    request.extensions_mut().insert(loader);

    next.run(request).await
}
