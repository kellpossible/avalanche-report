use axum::{extract, response::IntoResponse, Extension};
use forecast_spreadsheet::Size;
use http::{header, HeaderMap};
use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed_fl::fl;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};

use std::{collections::HashMap, sync::Arc};

use crate::i18n::I18nLoader;

#[derive(Serialize, Deserialize, Debug)]
pub struct Query {
    pub size: Size,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SizeBar {
    pub size: Size,
}

impl From<Query> for SizeBar {
    fn from(query: Query) -> Self {
        Self { size: query.size }
    }
}

const SVG_TEMPLATE: &str = include_str!("./size.svg");
const FILLED_COLOUR: &str = "#276fdcff";
const TRANSPARENT_COLOUR: &str = "#00000000";
static PATH_ID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"<rect\s*style="(?P<style>fill:(?P<fill>.*);)([^/])*id="(?P<id>.+)""#)
        .expect("Unable to compile svg path id regex")
});

static TEXT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"<text([^>])*id="(?P<id>.+)"(.|\s)*?<tspan(.|\s)*?>(?P<text>.*?)<[/]tspan>(.|\s)*?<[/]text>"#
    )
    .expect("Unable to compile svg text regex")
});

fn generate_svg(size_bar: SizeBar, i18n: Arc<FluentLanguageLoader>) -> String {
    let colour_map: HashMap<String, (Size, &str)> = enum_iterator::all::<Size>()
        .map(|size| {
            let colour = if size > size_bar.size {
                TRANSPARENT_COLOUR
            } else {
                FILLED_COLOUR
            };
            (format!("size{size}"), (size, colour))
        })
        .collect();
    let svg = PATH_ID_RE.replace_all(SVG_TEMPLATE, |captures: &Captures| {
        let id = captures.name("id").unwrap().as_str();
        let captured_string = captures.get(0).unwrap().as_str();
        if let Some((_size, colour)) = colour_map.get(id) {
            let style_group_string = captures.name("style").unwrap().as_str();
            captured_string.replace(style_group_string, &format!("fill:{colour};"))
        } else {
            captured_string.to_string()
        }
    });
    let text_map: HashMap<String, (Size, String)> = enum_iterator::all::<Size>()
        .map(|size| {
            (
                format!("size{size}text"),
                (
                    size,
                    fl!(&*i18n, "avalanche-size-n", size = size.to_string()).to_owned(),
                ),
            )
        })
        .collect();

    TEXT_RE
        .replace_all(&svg, |captures: &Captures| {
            let id = captures.name("id").unwrap().as_str();
            let captured_string = captures.get(0).unwrap().as_str();
            let original_text = captures.name("text").unwrap().as_str();

            let value = text_map.get(id).expect("Expected id to be present");
            let new_text = if value.0 == size_bar.size {
                &value.1
            } else {
                ""
            };
            captured_string.replace(original_text, new_text)
        })
        .to_string()
}

pub async fn svg_handler(
    extract::Query(query): extract::Query<Query>,
    Extension(i18n): Extension<I18nLoader>,
) -> axum::response::Result<impl IntoResponse> {
    let mut headers = HeaderMap::new();
    // headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    let size_bar = SizeBar::from(query);
    Ok((headers, generate_svg(size_bar, i18n)))
}
