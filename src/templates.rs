use std::{borrow::Cow, collections::HashMap, future, future::Future, pin::Pin, sync::Arc};

use axum::{
    extract::State,
    middleware::Next,
    response::{IntoResponse, Response},
    Extension,
};
use fluent::{types::FluentNumber, FluentValue};
use http::{header::CONTENT_TYPE, Request, StatusCode};
use rust_embed::{EmbeddedFile, RustEmbed};

use crate::{error::map_eyre_error, i18n::I18nLoader, AppState};

#[derive(RustEmbed)]
#[folder = "src/templates"]
struct EmbeddedTemplates;

#[derive(Clone)]
#[repr(transparent)]
pub struct Templates {
    pub reloader: Arc<minijinja_autoreload::AutoReloader>,
}

#[derive(Clone)]
#[repr(transparent)]
pub struct TemplatesWithContext {
    pub environment: Arc<minijinja::Environment<'static>>,
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

            // RustEmbed only loads from files in debug mode (unless the debug embed feature is
            // enabled).
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

fn jinja_to_fluent_args<'source>(
    args: minijinja::value::Value,
) -> Result<HashMap<String, FluentValue<'source>>, minijinja::Error> {
    match args.kind() {
        minijinja::value::ValueKind::Map => {
            args.try_iter()?.map(|key| {
                match key.kind() {
                    minijinja::value::ValueKind::String => {},
                    kind => return Err(
                        minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            format!("Invalid argument map key kind {kind} for {key}. Expected String.")
                        )
                    )
                }
                let value = args.get_item(&key)?;
                let fluent_value = match value.kind() {
                    minijinja::value::ValueKind::String => {
                        FluentValue::String(Cow::Owned(value.to_string()))
                    }
                    minijinja::value::ValueKind::Number => {
                        let fluent_number: FluentNumber = value.to_string().parse().map_err(|error| {
                            minijinja::Error::new(
                                minijinja::ErrorKind::InvalidOperation,
                                format!("Unable to parse value number as fluent number for {value}")
                            ).with_source(error)
                        })?;
                        FluentValue::Number(fluent_number)
                    }
                    kind => return Err(
                        minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            format!("Invalid argument map value kind {kind} for {value}. Expected String or Number.")
                        )
                    )
                };

                Ok((key.to_string(), fluent_value))
            }).collect()
        },
        kind => return Err(
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("Invalid argument type {kind} for {args}. Expected a Map.")
            )
        )
    }
}

/// Middleware that provides access to all available templates with context injected.
pub async fn middleware<B>(
    State(state): State<AppState>,
    Extension(i18n): Extension<I18nLoader>,
    mut request: Request<B>,
    next: Next<B>,
) -> axum::response::Result<impl IntoResponse> {
    let mut environment = (*state.templates.reloader.acquire_env().map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error acquiring template environment: {error}"),
        )
    })?)
    .clone();
    let language = i18n
        .current_languages()
        .get(0)
        .ok_or_else(|| eyre::eyre!("No current language"))
        .map_err(map_eyre_error)?
        .to_string();
    let i18n_fl = i18n.clone();
    let i18n_fla = i18n.clone();
    environment.add_function("fl", move |message_id: &str| i18n_fl.get(message_id));
    environment.add_function(
        "fla",
        move |message_id: &str, args: minijinja::value::Value| {
            Ok(i18n_fla.get_args(message_id, jinja_to_fluent_args(args)?))
        },
    );
    environment.add_function("ansi_to_html", |ansi_string: &str| {
        ansi_to_html::convert_escaped(ansi_string).map_err(|error| {
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("Error while converting ANSI string to HTML"),
            )
            .with_source(error)
        })
    });
    environment.add_global("LANGUAGE", language);
    environment.add_global("URI", request.uri().to_string());
    environment.add_global("PATH", request.uri().path().to_string());
    request.extensions_mut().insert(TemplatesWithContext {
        environment: Arc::new(environment),
    });

    Ok(next.run(request).await)
}

/// Render a template into a response, `Content-Type` header is guessed using the file extension of
/// the template name.
pub fn render<'env>(
    template: &minijinja::Template<'env>,
    ctx: &dyn erased_serde::Serialize,
) -> axum::response::Result<Response> {
    let mime = mime_guess::from_path(template.name()).first();

    let builder = Response::builder();
    let builder = if let Some(mime) = mime {
        builder.header(CONTENT_TYPE, mime.to_string())
    } else {
        builder
    };

    Ok(builder
        .body(template.render(ctx).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?
        .into_response())
}

// TODO: this code might be useful in the future for /** routes we can have a handler selects the
// appropriate template automatically if there is one.
//
// pub fn template_key(uri: &Uri) -> Option<String> {
//     let mut template_path: String = uri.path().to_string();
//     if template_path.is_empty() {
//         return None;
//     }
//     let first = template_path
//         .chars()
//         .next()
//         .expect("expected to be at least one character");
//     if first == '/' {
//         template_path.remove(0);
//     }
//
//     if template_path.is_empty() {
//         template_path.push_str("index");
//     } else {
//         let last = template_path
//             .chars()
//             .last()
//             .expect("expected to be at least one character");
//         if last == '/' {
//             template_path.push_str("index");
//         }
//     }
//     template_path.push_str(".html");
//     Some(template_path)
// }
//
// #[repr(transparent)]
// pub struct TemplateKey(String);
//
// #[async_trait]
// impl FromRequestParts<AppState> for TemplateKey {
//     type Rejection = (StatusCode, String);
//
//     async fn from_request_parts(
//         parts: &mut Parts,
//         state: &AppState,
//     ) -> Result<Self, Self::Rejection> {
//         let key = template_key(&parts.uri).ok_or_else(|| {
//             (
//                 StatusCode::INTERNAL_SERVER_ERROR,
//                 format!("Unable to parse URI {} as template key", parts.uri),
//             )
//         })?;
//         state
//             .templates
//             .reloader
//             .acquire_env()
//             .map_err(|error| {
//                 (
//                     StatusCode::INTERNAL_SERVER_ERROR,
//                     format!("Error acquiring template environment: {error}"),
//                 )
//             })?
//             .get_template(&key)
//             .map(|_| TemplateKey(key))
//             .map_err(|error| {
//                 (
//                     StatusCode::INTERNAL_SERVER_ERROR,
//                     format!("Error gettting a template for this route: {error}"),
//                 )
//             })
//     }
// }

/// Create a handler which renders a response using the template at `template_key`.
pub fn create_handler(
    template_key: &str,
) -> impl (Fn(
    Extension<TemplatesWithContext>,
) -> Pin<Box<dyn Future<Output = axum::response::Result<Response>> + Send>>)
       + Clone
       + Send
       + 'static {
    fn render_impl(
        templates: &TemplatesWithContext,
        template_key: &str,
    ) -> axum::response::Result<Response> {
        let template = templates
            .environment
            .get_template(&template_key)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        render(&template, &())
    }
    let template_key: String = template_key.to_owned();
    move |Extension(templates): Extension<TemplatesWithContext>| {
        let result = render_impl(&templates, &template_key);
        Box::pin(future::ready(result))
    }
}
