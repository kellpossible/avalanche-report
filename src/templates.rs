use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    future::{self, Future},
    pin::Pin,
    sync::Arc,
};

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
    Extension,
};
use eyre::Context;
use fluent::{types::FluentNumber, FluentValue};
use http::{header::CONTENT_TYPE, StatusCode};
use minijinja::{
    value::{Value, ValueKind},
    Error, ErrorKind,
};
use pulldown_cmark::{Event, Tag, TagEnd};
use rust_embed::{EmbeddedFile, RustEmbed};
use uuid::Uuid;

use crate::{
    error::map_eyre_error,
    i18n::{order_languages, ordered_language_display_names, I18nLoader},
    AppState,
};

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

impl TemplatesWithContext {
    pub fn render(
        &self,
        name: &str,
        context: &dyn erased_serde::Serialize,
    ) -> eyre::Result<Response> {
        render(&self.environment, name, context)
    }
}

impl Templates {
    pub fn initialize(options: &'static crate::options::Templates) -> eyre::Result<Self> {
        let reloader = minijinja_autoreload::AutoReloader::new(|notifier| {
            let mut environment = minijinja::Environment::new();
            environment.set_loader(|name: &str| {
                if let Some(directory) = &options.directory {
                    let name_sanitized = name.replace("..", "");
                    let path = directory.join(name_sanitized);
                    if path.exists() {
                        return Ok(Some(std::fs::read_to_string(path).map_err(|error| {
                            Error::new(
                                ErrorKind::TemplateNotFound,
                                format!("Error loading template {name}: {error}"),
                            )
                        })?));
                    }
                }

                Option::transpose(EmbeddedTemplates::get(name).map(|file: EmbeddedFile| {
                    String::from_utf8(file.data.to_vec()).map_err(|error| {
                        Error::new(
                            ErrorKind::SyntaxError,
                            format!("Template {name} is not valid UTF-8: {error}"),
                        )
                    })
                }))
            });

            // RustEmbed only loads from files in debug mode (unless the debug embed feature is
            // enabled).
            #[cfg(debug_assertions)]
            {
                notifier.watch_path("src/templates", true);
            }
            if let Some(path) = &options.directory {
                notifier.watch_path(path, true);
            }
            Ok(environment)
        });
        Ok(Self {
            reloader: Arc::new(reloader),
        })
    }
}

fn jinja_to_fluent_args<'source>(
    args: Value,
) -> Result<HashMap<String, FluentValue<'source>>, Error> {
    match args.kind() {
        ValueKind::Map => {
            args.try_iter()?.map(|key| {
                match key.kind() {
                    ValueKind::String => {},
                    kind => return Err(
                        Error::new(
                            ErrorKind::InvalidOperation,
                            format!("Invalid argument map key kind {kind} for {key}. Expected String.")
                        )
                    )
                }
                let value = args.get_item(&key)?;
                let fluent_value = match value.kind() {
                    ValueKind::String => {
                        FluentValue::String(Cow::Owned(value.to_string()))
                    }
                    ValueKind::Number => {
                        let fluent_number: FluentNumber = value.to_string().parse().map_err(|error| {
                            Error::new(
                                ErrorKind::InvalidOperation,
                                format!("Unable to parse value number as fluent number for {value}")
                            ).with_source(error)
                        })?;
                        FluentValue::Number(fluent_number)
                    }
                    kind => return Err(
                        Error::new(
                            ErrorKind::InvalidOperation,
                            format!("Invalid argument map value kind {kind} for {value}. Expected String or Number.")
                        )
                    )
                };

                Ok((key.to_string(), fluent_value))
            }).collect()
        },
        kind => return Err(
            Error::new(
                ErrorKind::InvalidOperation,
                format!("Invalid argument type {kind} for {args}. Expected a Map.")
            )
        )
    }
}

/// Convert a [Value] into a query string: e.g.
/// `param=something&other_param=5` This supports a `Map<String, Value>`, and a `Seq<Seq<Value>>`
/// (where the length of the inner `Seq` is 2, the first element is `String` and the second element
/// is `Value`).
fn querystring(query: Value) -> Result<minijinja::value::Value, Error> {
    let query: Vec<String> = match query.kind() {
        ValueKind::Seq => {
            query.as_seq().expect("expected sequence").iter().map(|value| {
                let tuple = value.as_seq().ok_or_else(|| Error::new(minijinja::ErrorKind::InvalidOperation, format!("Expected Seq of Seq, but found Seq of {:?}", value.kind())))?;
                let mut iter = tuple.iter();
                let key_value = iter.next().ok_or_else(|| Error::new(minijinja::ErrorKind::InvalidOperation, "Expected Seq of Seq, inner Seq (a tuple of key and value) cannot be empty"))?;
                let key = key_value.as_str().ok_or_else(|| Error::new(minijinja::ErrorKind::InvalidOperation, format!("Expected Seq of Seq, inner Seq first element to be a String, instead found {key_value:?}")))?;
                let value = iter.next().ok_or_else(|| Error::new(minijinja::ErrorKind::InvalidOperation, "Expected Seq of Seq, inner Seq (a tuple of key and value) must contain a second element (the value)"))?;

                let extra_values: Vec<Value> = iter.collect();
                if !extra_values.is_empty() {
                    return Err(Error::new(minijinja::ErrorKind::InvalidOperation, format!("Expected Seq of Seq, inner Seq (a tuple of key and value) must be of length 2, found {extra_values:?}")));
                }
                Ok::<_, Error>(format!("{key}={value}"))
            }).collect::<Result<_, Error>>()?
        }
        ValueKind::None | ValueKind::Undefined => Vec::new(),
        ValueKind::Map => query
            .try_iter()?
            .filter_map(|key| {
                Result::transpose(query
                    .get_item(&key)
                    .map(|value| {
                        match value.kind() {
                            ValueKind::Undefined | ValueKind::None => None,
                            _ => Some(format!("{}={}", key.to_string(), urlencoding::encode(&value.to_string()))),
                        }
                    }))
            })
            .collect::<Result<_, Error>>()?,
        kind => {
            return Err(Error::new(
                ErrorKind::InvalidOperation,
                format!("Expected map, found {kind:?}"),
            ));
        }
    };

    Ok(query.join("&").into())
}

pub fn mapinsert(map: Value, key: String, value: Value) -> Result<Value, Error> {
    match map.kind() {
        ValueKind::Map => {
            let mut map: BTreeMap<String, Value> = map
                .try_iter()?
                .map(|key| {
                    let value = map.get_item(&key)?;
                    let key = key
                        .as_str()
                        .ok_or_else(|| {
                            Error::new(
                                ErrorKind::InvalidOperation,
                                format!("Key must be a string, found a {key:?}"),
                            )
                        })?
                        .to_owned();
                    Ok((key, value))
                })
                .collect::<Result<_, Error>>()?;
            map.insert(key, value);
            Ok(Value::from_serializable(&map))
        }
        ValueKind::None | ValueKind::Undefined => Ok(map),
        kind => Err(Error::new(
            ErrorKind::InvalidOperation,
            format!("Unsupported query value type: {kind:?}"),
        )),
    }
}

pub fn mapremove(map: Value, key: Cow<'_, str>) -> Result<Value, Error> {
    match map.kind() {
        ValueKind::Map => {
            let mut map: BTreeMap<String, Value> = map
                .try_iter()?
                .map(|key| {
                    let value = map.get_item(&key)?;
                    let key = key
                        .as_str()
                        .ok_or_else(|| {
                            Error::new(
                                ErrorKind::InvalidOperation,
                                format!("Key must be a string, found a {key:?}"),
                            )
                        })?
                        .to_owned();
                    Ok((key, value))
                })
                .collect::<Result<_, Error>>()?;
            map.remove(&*key);
            Ok(Value::from_serializable(&map))
        }
        ValueKind::None | ValueKind::Undefined => Ok(map),
        kind => Err(Error::new(
            ErrorKind::InvalidOperation,
            format!("Unsupported query value type: {kind:?}"),
        )),
    }
}

/// Middleware that provides access to all available templates with context injected.
pub async fn middleware(
    State(state): State<AppState>,
    Extension(i18n): Extension<I18nLoader>,
    mut request: Request,
    next: Next,
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
        .clone();

    let language_short = language.language.to_string();
    let language_full = language.to_string();

    let i18n_fl = i18n.clone();
    let i18n_fl_md = i18n.clone();
    let i18n_negotiate_translation = i18n.clone();

    let language_display_names = minijinja::value::Value::from_serializable(
        &ordered_language_display_names(&state.options.default_language_order),
    );

    environment.add_function("translated_string", move |translations: Value| {
        tracing::debug!("translations: {translations:?}");

        if translations.len().unwrap_or(0) == 0 {
            return Err(minijinja::Error::new(
                minijinja::ErrorKind::MissingArgument,
                format!(
                    "No translations are provided in the translations argument: {translations:?}"
                ),
            ));
        }

        let available_languages: Vec<unic_langid::LanguageIdentifier> = translations
            .try_iter()?
            .map(|key| {
                let key = match key.as_str() {
                    Some(key) => key,
                    None => {
                        return Err(minijinja::Error::new(
                            ErrorKind::InvalidOperation,
                            "key is not a string",
                        ))
                    }
                };

                let language: unic_langid::LanguageIdentifier = key.parse().map_err(|e| {
                    minijinja::Error::new(
                        ErrorKind::InvalidOperation,
                        format!("unable to parse key as language: {e:?}"),
                    )
                })?;

                Ok(language)
            })
            .collect::<Result<_, minijinja::Error>>()?;

        let available_languages = order_languages(
            available_languages,
            &state.options.default_language_order,
            |al, l| al == l,
        );

        let requested_languages = i18n_negotiate_translation.current_languages();

        let selected_languages = fluent_langneg::negotiate_languages(
            &requested_languages,
            &available_languages,
            Some(available_languages.first().ok_or_else(|| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "Unexpected error: expected at least one langauage to be available",
                )
            })?),
            fluent_langneg::NegotiationStrategy::Filtering,
        );

        let selected_language = match selected_languages.first() {
            Some(language) => language.to_string(),
            None => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "Unexpected error, no language selected",
                ))
            }
        };
        // Return Value because this value is optional.
        translations.get_item(&Value::from(selected_language))
    });
    // Render a fluent message.
    environment.add_function("fl", move |message_id: &str, args: Option<Value>| {
        Ok(if let Some(args) = args {
            i18n_fl.get_args(message_id, jinja_to_fluent_args(args)?)
        } else {
            i18n_fl.get(message_id)
        })
    });
    // Render fluent message as markdown.
    //
    // Available options:
    // + use_isolating - sets the
    //   https://docs.rs/fluent/latest/fluent/bundle/struct.FluentBundle.html#method.set_use_isolating
    //   flag for this message. `true` by default.
    // + strip_paragraph - An option to strip paragraph tags from the parsed markdown. `true` by
    //   default.
    environment.add_function(
        "fl_md",
        move |message_id: &str, args: Option<Value>, options: Option<Value>| {
            let options = options.unwrap_or_default();

            let message = if let Some(args) = args {
                let use_isolating = if let Ok(value) = options.get_attr("use_isolating") {
                    match value.kind() {
                        ValueKind::Bool => value.is_true(),
                        ValueKind::Undefined => false,
                        invalid => {
                            return Err(minijinja::Error::new(
                                ErrorKind::InvalidOperation,
                                format!("Invalid value kind for option use_isolating: {invalid}"),
                            ))
                        }
                    }
                } else {
                    true
                };
                if !use_isolating {
                    i18n_fl_md.set_use_isolating(false);
                }
                let message = i18n_fl_md.get_args(message_id, jinja_to_fluent_args(args)?);
                if !use_isolating {
                    i18n_fl_md.set_use_isolating(true);
                }
                message
            } else {
                i18n_fl_md.get(message_id)
            };

            let parser = pulldown_cmark::Parser::new(&message);

            let parser: Box<dyn Iterator<Item = Event>> = match options.get_attr("strip_paragraph")
            {
                Ok(value) if !value.is_undefined() && !value.is_true() => {
                    Box::new(parser.into_iter())
                }
                _ => Box::new(parser.filter_map(|event| match event {
                    Event::Start(Tag::Paragraph) => None,
                    Event::End(TagEnd::Paragraph) => None,
                    _ => Some(event),
                })),
            };

            let mut html = String::new();
            pulldown_cmark::html::push_html(&mut html, parser);
            Ok(Value::from_safe_string(html))
        },
    );
    environment.add_function("ansi_to_html", |ansi_string: &str| {
        ansi_to_html::convert_with_opts(ansi_string, &ansi_to_html::Opts::default()).map_err(
            |error| {
                Error::new(
                    ErrorKind::InvalidOperation,
                    "Error while converting ANSI string to HTML".to_owned(),
                )
                .with_source(error)
            },
        )
    });
    let uri = request.uri();
    let query_value: Value = uri
        .query()
        .and_then(|query| match serde_urlencoded::from_str(query) {
            Ok(ok) => Some(ok),
            Err(error) => {
                tracing::error!("Error parsing uri into QUERY variable: {error}");
                None
            }
        })
        .unwrap_or(().into());
    environment.add_function("uuid", || Uuid::new_v4().to_string());
    environment.add_filter("md", |value: Value| {
        if value.is_none() || value.is_undefined() {
            return value;
        }
        if let Some(string) = value.as_str() {
            let parser = pulldown_cmark::Parser::new(string);
            let mut html = String::new();
            pulldown_cmark::html::push_html(&mut html, parser);
            Value::from_safe_string(html)
        } else {
            Value::from(())
        }
    });
    environment.add_filter("querystring", querystring);
    environment.add_filter("mapinsert", mapinsert);
    environment.add_filter("mapremove", mapremove);
    environment.add_global("LANGUAGE_SHORT", language_short);
    environment.add_global("LANGUAGE", language_full);
    environment.add_global("LANGUAGE_DISPLAY_NAMES", language_display_names);
    environment.add_global("URI", uri.to_string());
    environment.add_global("PATH", uri.path().to_string());
    environment.add_global("QUERY", query_value);
    request.extensions_mut().insert(TemplatesWithContext {
        environment: Arc::new(environment),
    });

    Ok(next.run(request).await)
}

/// Render a template into a response. `Content-Type` header is guessed using the file extension of
/// the template name.
pub fn render<'env>(
    environment: &minijinja::Environment<'env>,
    name: &str,
    ctx: &dyn erased_serde::Serialize,
) -> eyre::Result<Response> {
    let template = environment.get_template(name)?;
    let mime = mime_guess::from_path(template.name()).first();

    let builder = Response::builder();
    let builder = if let Some(mime) = mime {
        builder.header(CONTENT_TYPE, mime.to_string())
    } else {
        builder
    };

    Ok(builder
        .body(
            template
                .render(ctx)
                .wrap_err_with(|| format!("Error rendering template {name:?}"))?,
        )?
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
//                     format!("Error getting a template for this route: {error}"),
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
        Ok(templates
            .render(template_key, &())
            .map_err(map_eyre_error)?)
    }
    let template_key: String = template_key.to_owned();
    move |Extension(templates): Extension<TemplatesWithContext>| {
        let result = render_impl(&templates, &template_key);
        Box::pin(future::ready(result))
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use minijinja::value::Value;

    use super::querystring;
    #[test]
    fn test_query_filter() {
        let mut map = HashMap::new();
        map.insert("test", 5);
        map.insert("test2", 22);

        let value = Value::from(map);
        insta::assert_json_snapshot!(&value, @r###"
        {
          "test": 5,
          "test2": 22
        }
        "###);
        let result_value = querystring(value).unwrap();
        let result_value_string = result_value.as_str().unwrap().to_owned();
        assert_eq!("test=5&test2=22", result_value_string);
    }
}
