use axum::{
    http::{HeaderMap, HeaderValue, Request},
    middleware::Next,
    response::Response,
};
use i18n_embed::{
    fluent::{fluent_language_loader, FluentLanguageLoader, NegotiationStrategy},
    LanguageLoader,
};
use once_cell::sync::Lazy;
use rust_embed::RustEmbed;
use std::sync::Arc;

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

pub static LOADER: Lazy<I18nLoader> = Lazy::new(|| Arc::new(fluent_language_loader!()));

pub fn load_languages() -> eyre::Result<()> {
    let languages: String = display_languages(&LOADER.available_languages(&Localizations)?);
    LOADER.load_available_languages(&Localizations)?;
    tracing::info!("Localizations loaded. Available languages: {languages}");
    Ok(())
}

pub async fn middleware<B>(headers: HeaderMap, mut request: Request<B>, next: Next<B>) -> Response {
    tracing::debug!("{headers:?}");
    let loader: I18nLoader = if let Some(accept_language) =
        headers.get("Accept-Language").map(parse_accept_language)
    {
        tracing::debug!("AcceptLanguage: {accept_language}");
        let loader = Arc::new(
            LOADER.select_languages_negotiate(&accept_language.0, NegotiationStrategy::Filtering),
        );
        request.extensions_mut().insert(accept_language);
        loader
    } else {
        LOADER.clone()
    };

    request.extensions_mut().insert(loader);

    next.run(request).await
}
