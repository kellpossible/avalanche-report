use axum::response::{Html, IntoResponse};
use html_builder::{Buffer, Html5};

use crate::{error::handle_eyre_error, i18n::I18nLoader};

use super::{script_src, stylesheet, ComponentNode};

#[derive(buildstructor::Builder)]
pub struct Base<'n> {
    body: &'n dyn ComponentNode,
    /// Set the language for the page.
    i18n: I18nLoader,
    head: Option<&'n dyn ComponentNode>,
    /// Appended to the end of the body after this component's scripts.
    body_scripts: Option<&'n dyn ComponentNode>,
}

impl<'n> Base<'n> {
    pub fn run(&self) -> eyre::Result<String> {
        let mut buf = Buffer::new();
        buf.doctype();
        let lang = self
            .i18n
            .current_languages()
            .get(0)
            .ok_or_else(|| eyre::eyre!("No current language"))?
            .clone();
        let mut html = buf.html().attr(&format!(r#"lang="{lang}""#));
        let mut head = html.head();
        head.meta().attr(r#"charset="UTF-8""#);
        head.meta()
            .attr(r#"name="viewport""#)
            .attr(r#"content="width=device-width, initial-scale=1.0""#);
        stylesheet(&mut head, "/dist/style.css");
        script_src(&mut head, "/dist/htmx.js");
        Option::transpose(self.head.map(|h| h.run(&mut head)))?;

        let mut body = html.body();
        self.body.run(&mut body)?;
        Option::transpose(self.body_scripts.map(|s| s.run(&mut body)))?;
        Ok(buf.finish())
    }
}

impl<'n> IntoResponse for &Base<'n> {
    fn into_response(self) -> axum::response::Response {
        match self.run() {
            Ok(html) => Html(html).into_response(),
            Err(error) => handle_eyre_error(error).into_response(),
        }
    }
}

impl<'n> IntoResponse for Base<'n> {
    fn into_response(self) -> axum::response::Response {
        (&self).into_response()
    }
}
