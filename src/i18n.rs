use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue},
    middleware::Next,
    response::Response,
};
use eyre::OptionExt;
use i18n_embed::{
    fluent::{fluent_language_loader, FluentLanguageLoader, NegotiationStrategy},
    AssetsMultiplexor, FileSystemAssets, I18nAssets, LanguageLoader, RustEmbedNotifyAssets,
};
use once_cell::sync::Lazy;
use once_cell::sync::OnceCell;
use rust_embed::RustEmbed;
use std::{any::Any, collections::HashMap, path::PathBuf, sync::Arc};
use time::OffsetDateTime;

use crate::{state::AppState, user_preferences::UserPreferences};

#[derive(RustEmbed)]
#[folder = "i18n/"]
pub struct LocalizationsEmbed;

pub static LOCALIZATIONS: OnceCell<AssetsMultiplexor> = OnceCell::new();

pub fn try_get_localizations() -> eyre::Result<&'static AssetsMultiplexor> {
    LOCALIZATIONS
        .get()
        .ok_or_eyre("LOCALIZATIONS have not yet been initialized")
}

pub static LANGUAGE_DISPLAY_NAMES: Lazy<HashMap<unic_langid::LanguageIdentifier, String>> =
    Lazy::new(|| {
        vec![
            ("en-UK", "English"),
            ("ka-GE", "ქართული"),
            ("bg-BG", "български"),
        ]
        .into_iter()
        .map(|(id, name)| (id.parse().unwrap(), name.to_owned()))
        .collect()
    });

#[derive(Clone, Debug)]
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

/// Negotiate which translated string ot use based on the user's requested languages.
pub fn negotiate_translated_string<'a>(
    requested_languages: &[unic_langid::LanguageIdentifier],
    default_language: &'a unic_langid::LanguageIdentifier,
    text: &'a HashMap<unic_langid::LanguageIdentifier, String>,
) -> Option<(&'a unic_langid::LanguageIdentifier, &'a str)> {
    let available_languages: Vec<_> = text.keys().collect();
    let selected = fluent_langneg::negotiate_languages(
        requested_languages,
        &available_languages,
        Some(&default_language),
        fluent_langneg::NegotiationStrategy::Filtering,
    );

    let first = selected.first();

    first.and_then(|first| text.get(first).map(|text| (**first, text.as_str())))
}

pub type I18nLoader = Arc<FluentLanguageLoader>;

/// Returns the loader, and a reload watcher (which we must hold for the duration of the program.
pub fn initialize(options: &crate::options::I18n) -> eyre::Result<(I18nLoader, Box<dyn Any>)> {
    let mut assets: Vec<Box<dyn I18nAssets + Send + Sync + 'static>> =
        vec![Box::new(RustEmbedNotifyAssets::<LocalizationsEmbed>::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("i18n"),
        ))];
    if let Some(directory) = &options.directory {
        if !directory.is_dir() {
            tracing::warn!("Specified i18n directory {directory:?} either does not exist or is not a valid directory");
        }
        assets.insert(
            0,
            Box::new(FileSystemAssets::try_new(directory)?.notify_changes_enabled(true)),
        );
    }
    LOCALIZATIONS
        .set(AssetsMultiplexor::new(assets))
        .map_err(|_| eyre::eyre!("Unable to set LOCALIZATIONS because it has already been set"))?;
    let localizations = try_get_localizations()?;

    let loader = Arc::new(fluent_language_loader!());
    let changed_loader = loader.clone();
    let watcher = localizations.subscribe_changed(std::sync::Arc::new(move || {
        if let eyre::Result::Err(error) = (|| {
            tracing::debug!("Reloading localizations detected change");
            changed_loader
                .reload(localizations)
                .map_err(eyre::Error::from)
        })() {
            tracing::error!("Error autoreloading localizations: {error:?}");
        }
    }))?;

    Ok((loader, Box::new(watcher)))
}

/// Create an ordered version of [`LANGUAGE_DISPLAY_NAMES`].
pub fn ordered_language_display_names(
    language_order: &[unic_langid::LanguageIdentifier],
) -> Vec<(unic_langid::LanguageIdentifier, String)> {
    order_languages(
        LANGUAGE_DISPLAY_NAMES.clone().into_iter().collect(),
        language_order,
        |(id, _), order_id| id == order_id,
    )
}

/// Order a vec of items according to the order specified in `language_order`, using the `eq` function to
/// match elements in `unordered` to those in `language_order`. Any items which have no match will
/// retain their original order, after any ordered items.
pub fn order_languages<T>(
    mut unordered: Vec<T>,
    language_order: &[unic_langid::LanguageIdentifier],
    eq: impl Fn(&T, &unic_langid::LanguageIdentifier) -> bool,
) -> Vec<T> {
    let mut ordered = Vec::new();
    for l in language_order {
        if let Some(i) = unordered.iter().position(|t| eq(t, l)) {
            ordered.push(unordered.remove(i));
        }
    }
    ordered.extend(unordered.into_iter());
    ordered
}

pub fn load_available_languages<'a>(
    loader: &I18nLoader,
    language_order: &[unic_langid::LanguageIdentifier],
) -> eyre::Result<()> {
    let localizations = try_get_localizations()?;
    let available_languages = loader.available_languages(&*localizations)?;
    let languages = order_languages(available_languages, language_order, |al, l| al == l);
    loader.load_languages(&*localizations, &languages)?;

    let languages_display: String = display_languages(&languages);
    tracing::debug!("Localizations loaded, languages: {languages_display}");
    Ok(())
}

pub async fn middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Response {
    let preferences: &UserPreferences = request
        .extensions()
        .get()
        .expect("Expected user_preferences middleware to be installed before this middleware");

    let accept_language = headers.get("Accept-Language").map(parse_accept_language);
    let requested_languages = preferences
        .lang
        .as_ref()
        .map(|lang| {
            let mut requested_languages = RequestedLanguages(vec![lang.clone()]);
            if let Some(accept_language) = &accept_language {
                requested_languages
                    .0
                    .extend(accept_language.0.iter().cloned())
            }
            requested_languages
        })
        .or(accept_language);

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

pub fn format_time(time: OffsetDateTime, i18n: &I18nLoader) -> String {
    let day = time.day();
    let month = time.month() as u8;
    let month_name = i18n.get(&format!("month-{month}"));
    let year = time.year();
    let hour = time.hour();
    let minute = time.minute();
    format!("{day} {month_name} {year} {hour:0>2}:{minute:0>2}")
}
