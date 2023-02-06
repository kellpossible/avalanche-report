use axum::{
    extract::{FromRef, State},
    http::{HeaderMap, HeaderValue, Request},
    middleware::Next,
    response::Response,
};
use i18n_embed::{
    fluent::{fluent_language_loader, FluentLanguageLoader, NegotiationStrategy},
    LanguageLoader,
};
use rust_embed::RustEmbed;
use std::sync::Arc;

use crate::state::AppState;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub struct AcceptLanguage(pub Vec<unic_langid::LanguageIdentifier>);

impl std::fmt::Display for AcceptLanguage {
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

fn parse_accept_language(accept_language: &HeaderValue) -> AcceptLanguage {
    AcceptLanguage(
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

pub async fn middleware<B>(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut request: Request<B>,
    next: Next<B>,
) -> Response {
    tracing::debug!("{headers:?}");
    let loader: I18nLoader =
        if let Some(accept_language) = headers.get("Accept-Language").map(parse_accept_language) {
            tracing::debug!("AcceptLanguage: {accept_language}");
            let loader = Arc::new(
                state
                    .i18n
                    .select_languages_negotiate(&accept_language.0, NegotiationStrategy::Filtering),
            );
            request.extensions_mut().insert(accept_language);
            loader
        } else {
            state.i18n.clone()
        };

    request.extensions_mut().insert(loader);

    next.run(request).await
}
