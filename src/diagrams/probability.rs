use std::collections::HashMap;

use axum::{extract, response::IntoResponse, Extension};
use http::{header, HeaderMap};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};

use crate::{forecasts::probability::Probability, i18n::I18nLoader};

pub struct ProbabilityBar {
    probability: Probability,
}

impl From<Query> for ProbabilityBar {
    fn from(query: Query) -> Self {
        Self {
            probability: query.probability,
        }
    }
}

const SVG_TEMPLATE: &str = include_str!("./probability.svg");
const FILLED_COLOUR: &str = "#276fdcff";
const TRANSPARENT_COLOUR: &str = "#00000000";
const DISABLED_COLOUR: &str = "#808080ff";
static PATH_ID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"<rect\s*style="(?P<style>fill:(?P<fill>.*);)([^/])*id="(?P<id>.+)""#)
        .expect("Unable to compile svg path id regex")
});

static TEXT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"<text([^>])*id="(?P<id>.+)"(.|\s)*?fill:(?P<fill>#000000)(.|\s)*?<tspan(.|\s)*?>(?P<text>.*?)<[/]tspan>(.|\s)*?<[/]text>"#
    )
    .expect("Unable to compile svg text regex")
});

fn generate_svg(probability_bar: ProbabilityBar, i18n: I18nLoader) -> String {
    let colour_map: HashMap<&'static str, (Probability, &str)> =
        enum_iterator::all::<Probability>()
            .map(|probability| {
                let colour = if probability == probability_bar.probability {
                    FILLED_COLOUR
                } else {
                    TRANSPARENT_COLOUR
                };
                (probability.id(), (probability, colour))
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
    let text_map: HashMap<String, (Probability, String)> = enum_iterator::all::<Probability>()
        .map(|probability| {
            (
                format!("{}_text", probability.id()),
                (
                    probability,
                    i18n.get(&format!("avalanche-probability-{}", probability.id())),
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
            let new_text = &value.1;

            let original_fill = captures.name("fill").unwrap().as_str();
            let new_fill = if value.0 == probability_bar.probability {
                &original_fill
            } else {
                DISABLED_COLOUR
            };

            captured_string
                .replace(original_text, new_text)
                .replace(original_fill, new_fill)
        })
        .to_string()
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Query {
    pub probability: Probability,
}

pub async fn svg_handler(
    extract::Query(query): extract::Query<Query>,
    Extension(i18n): Extension<I18nLoader>,
) -> axum::response::Result<impl IntoResponse> {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    let probability_bar = ProbabilityBar::from(query);
    Ok((headers, generate_svg(probability_bar, i18n)))
}
