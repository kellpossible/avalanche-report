use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{FromRequestParts, State},
    headers::ContentType,
    middleware::Next,
    response::{IntoResponse, Response},
    Extension,
};
use http::{header::CONTENT_TYPE, request::Parts, Request, StatusCode, Uri};
use rust_embed::{EmbeddedFile, RustEmbed};
use serde::Serialize;

use crate::{error::handle_eyre_error, i18n::I18nLoader, AppState};

#[derive(RustEmbed)]
#[folder = "src/templates"]
struct EmbeddedTemplates;

#[derive(Clone)]
#[repr(transparent)]
pub struct Templates {
    reloader: Arc<minijinja_autoreload::AutoReloader>,
}

#[derive(Clone)]
#[repr(transparent)]
pub struct TemplatesWithContext {
    environment: Arc<minijinja::Environment<'static>>,
}

impl Templates {
    pub fn initialize() -> eyre::Result<Self> {
        let reloader = minijinja_autoreload::AutoReloader::new(|notifier| {
            let mut environment = minijinja::Environment::new();
            environment.set_source(minijinja::Source::with_loader(|name: &str| {
                Option::transpose(EmbeddedTemplates::get(name).map(|file: EmbeddedFile| {
                    String::from_utf8(file.data.to_vec()).map_err(|error| {
                        minijinja::Error::new(
                            minijinja::ErrorKind::SyntaxError,
                            format!("Template {name} is not valid UTF-8: {error}"),
                        )
                    })
                }))
            }));

            #[cfg(debug_assertions)]
            {
                notifier.watch_path("src/templates", true);
            }
            Ok(environment)
        });
        Ok(Self {
            reloader: Arc::new(reloader),
        })
    }
}

/// Middleware that provides access to all available templates.
pub async fn middleware<B>(
    State(state): State<AppState>,
    Extension(i18n): Extension<I18nLoader>,
    mut request: Request<B>,
    next: Next<B>,
) -> Response {
    let mut environment = (*state.templates.reloader.acquire_env().unwrap()).clone();
    let i18n = i18n.clone();
    environment.add_function("fl", move |message_id: &str| i18n.get(message_id));
    request.extensions_mut().insert(TemplatesWithContext {
        environment: Arc::new(environment),
    });

    next.run(request).await
}

#[repr(transparent)]
pub struct PathTemplate {
    template: Option<minijinja::Template<'static>>,
}

pub fn template_key(uri: &Uri) -> Option<String> {
    let mut template_path: String = uri.path().to_string();
    if template_path.is_empty() {
        return None;
    }
    let first = template_path
        .chars()
        .next()
        .expect("expected to be at least one character");
    if first == '/' {
        template_path.remove(0);
    }
    let _ = template_path.replace(':', "_");
    template_path.push_str(".html");
    Some(template_path)
}

#[repr(transparent)]
pub struct TemplateKey(String);

#[async_trait]
impl FromRequestParts<AppState> for TemplateKey {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let key = template_key(&parts.uri).ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Unable to parse URI {} as template key", parts.uri),
            )
        })?;
        state
            .templates
            .reloader
            .acquire_env()
            .unwrap()
            .get_template(&key)
            .map(|_| TemplateKey(key))
            .map_err(|error| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Error gettting a template for this route: {error}"),
                )
            })
    }
}

// TODO: Create an https://github.com/dtolnay/erased-serde version of this
fn render<'env, S: Serialize>(
    template: &minijinja::Template<'env>,
    ctx: S,
) -> axum::response::Result<impl IntoResponse> {
    let mime = mime_guess::from_path(template.name()).first();

    let builder = Response::builder();
    let builder = if let Some(mime) = mime {
        builder.header(CONTENT_TYPE, mime.to_string())
    } else {
        builder
    };

    Ok(builder
        .body(template.render(ctx).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?)
}

#[tracing::instrument(skip_all)]
pub async fn handler(
    TemplateKey(key): TemplateKey,
    // Extension(i18n): Extension<I18nLoader>,
    Extension(templates): Extension<TemplatesWithContext>,
) -> axum::response::Result<impl IntoResponse> {
    let template = templates
        .environment
        .get_template(&key)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    tracing::debug!("Using template {}", template.name());
    render(&template, ())
}
