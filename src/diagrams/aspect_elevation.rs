use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use axum::{
    extract::Query,
    http::{header, HeaderMap},
    response::IntoResponse,
};
use eyre::Context;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use resvg::{tiny_skia, usvg};
use serde::Deserialize;
use usvg_text_layout::{fontdb, TreeTextToPath};

use crate::error::{map_eyre_error, map_std_error};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Aspect {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl Aspect {
    fn id(&self) -> &'static str {
        match self {
            Aspect::N => "n",
            Aspect::NE => "ne",
            Aspect::E => "e",
            Aspect::SE => "se",
            Aspect::S => "s",
            Aspect::SW => "sw",
            Aspect::W => "w",
            Aspect::NW => "nw",
        }
    }

    #[cfg(test)]
    fn enumerate() -> &'static [Self] {
        &[
            Aspect::N,
            Aspect::NE,
            Aspect::E,
            Aspect::SE,
            Aspect::S,
            Aspect::SW,
            Aspect::W,
            Aspect::NW,
        ]
    }
}

impl FromStr for Aspect {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "N" => Self::N,
            "NE" => Self::NE,
            "E" => Self::E,
            "SE" => Self::SE,
            "S" => Self::S,
            "SW" => Self::SW,
            "W" => Self::W,
            "NW" => Self::NW,
            _ => return Err(eyre::eyre!("Unable to parse Aspect {:?}", s)),
        })
    }
}

#[derive(Debug, Default)]
pub struct AspectElevation {
    high_alpine: HashSet<Aspect>,
    high_alpine_text: Option<String>,
    alpine: HashSet<Aspect>,
    alpine_text: Option<String>,
    sub_alpine: HashSet<Aspect>,
    sub_alpine_text: Option<String>,
}

fn comma_separated_to_vec(comma_separated: String) -> eyre::Result<HashSet<Aspect>> {
    comma_separated
        .split(',')
        .into_iter()
        .filter_map(|aspect_str| {
            let aspect_str = aspect_str.trim();
            if aspect_str.is_empty() {
                None
            } else {
                Some(aspect_str.parse::<Aspect>())
            }
        })
        .collect()
}

impl TryFrom<AspectElevationQuery> for AspectElevation {
    type Error = eyre::Error;

    fn try_from(query: AspectElevationQuery) -> Result<Self, Self::Error> {
        let high_alpine = query
            .high_alpine
            .map(comma_separated_to_vec)
            .unwrap_or(Ok(HashSet::default()))
            .wrap_err("Error deserializing high_alpine")?;
        let alpine = query
            .alpine
            .map(comma_separated_to_vec)
            .unwrap_or(Ok(HashSet::default()))
            .wrap_err("Error deserializing alpine")?;
        let sub_alpine = query
            .sub_alpine
            .map(comma_separated_to_vec)
            .unwrap_or(Ok(HashSet::default()))
            .wrap_err("Error deserializing sub_alpine")?;
        Ok(Self {
            high_alpine,
            high_alpine_text: query.high_alpine_text,
            alpine,
            alpine_text: query.alpine_text,
            sub_alpine,
            sub_alpine_text: query.sub_alpine_text,
        })
    }
}

#[derive(Deserialize, Debug)]
pub struct AspectElevationQuery {
    high_alpine: Option<String>,
    high_alpine_text: Option<String>,
    alpine: Option<String>,
    alpine_text: Option<String>,
    sub_alpine: Option<String>,
    sub_alpine_text: Option<String>,
}

const SVG_TEMPLATE: &str = include_str!("./aspect_elevation.svg");
const FILLED_COLOUR: &str = "#276fdcff";
static PATH_ID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"<path\s*style="(?P<fill>fill:(?P<colour>#ffffff);)([^/])*id="(?P<id>[a-z\-]+)"\s*[/]>"#,
    )
    .expect("Unable to compile svg path id regex")
});

static TEXT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"<text([^/])*id="(?P<id>[a-z\-]+)"(.|\s)*?<tspan(.|\s)*?>(?P<text>.*?)<[/]tspan>(.|\s)*?<[/]text>"#
    )
    .expect("Unable to compile svg text regex")
});

fn generate_svg(aspect_elevation: AspectElevation) -> String {
    let high_alpine_ids = aspect_elevation.high_alpine.iter().map(|aspect| {
        let id = aspect.id();
        format!("high-alpine-{id}")
    });
    let alpine_ids = aspect_elevation.alpine.iter().map(|aspect| {
        let id = aspect.id();
        format!("alpine-{id}")
    });
    let sub_alpine_ids = aspect_elevation.sub_alpine.iter().map(|aspect| {
        let id = aspect.id();
        format!("sub-alpine-{id}")
    });

    let colour_map: HashMap<String, &str> = high_alpine_ids
        .chain(alpine_ids)
        .chain(sub_alpine_ids)
        .map(|id| (id, FILLED_COLOUR))
        .collect();

    let svg = PATH_ID_RE.replace_all(SVG_TEMPLATE, |captures: &Captures| {
        let id = captures.name("id").unwrap().as_str();
        let captured_string = captures.get(0).unwrap().as_str();
        if let Some(colour) = colour_map.get(id) {
            let fill_group_string = captures.name("fill").unwrap().as_str();
            captured_string.replace(fill_group_string, &format!("fill:{colour};"))
        } else {
            captured_string.to_string()
        }
    });

    TEXT_RE
        .replace_all(&svg, |captures: &Captures| {
            let id = captures.name("id").unwrap().as_str();
            let captured_string = captures.get(0).unwrap().as_str();
            let original_text = captures.name("text").unwrap().as_str();

            match id {
                "high-alpine-text" => {
                    if let Some(high_alpine_text) = &aspect_elevation.high_alpine_text {
                        captured_string.replace(original_text, &high_alpine_text)
                    } else {
                        captured_string.to_string()
                    }
                }
                "alpine-text" => {
                    if let Some(alpine_text) = &aspect_elevation.alpine_text {
                        captured_string.replace(original_text, &alpine_text)
                    } else {
                        captured_string.to_string()
                    }
                }
                "sub-alpine-text" => {
                    if let Some(sub_alpine_text) = &aspect_elevation.sub_alpine_text {
                        captured_string.replace(original_text, &sub_alpine_text)
                    } else {
                        captured_string.to_string()
                    }
                }
                _ => captured_string.to_string(),
            }
        })
        .to_string()
}

pub async fn svg_handler(
    Query(aspect_elevation_query): Query<AspectElevationQuery>,
) -> axum::response::Result<impl IntoResponse> {
    let mut headers = HeaderMap::new();
    // headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    let aspect_elevation =
        AspectElevation::try_from(aspect_elevation_query).map_err(map_eyre_error)?;
    Ok((headers, generate_svg(aspect_elevation)))
}

const FONT_DATA: &[u8] = include_bytes!("./fonts/noto/NotoSans-RegularWithGeorgian.ttf");
static FONT_DB: Lazy<fontdb::Database> = Lazy::new(|| {
    let mut db = fontdb::Database::new();
    db.load_font_data(FONT_DATA.to_vec());
    db.set_sans_serif_family("Noto Sans");
    db
});

fn generate_png(aspect_elevation: AspectElevation) -> eyre::Result<Vec<u8>> {
    let svg = generate_svg(aspect_elevation);
    let options = usvg::Options::default();
    let mut tree = usvg::Tree::from_str(&svg, &options)?;
    tree.convert_text(&*FONT_DB);
    let pixmap_size = tree.size.to_screen_size();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
        .ok_or_else(|| eyre::eyre!("Unable to create pixmap"))?;
    resvg::render(
        &tree,
        usvg::FitTo::Original,
        Default::default(),
        pixmap.as_mut(),
    )
    .ok_or_else(|| eyre::eyre!("Error rendering svg"))?;
    pixmap.encode_png().map_err(eyre::Error::from)
}

pub async fn png_handler(
    Query(aspect_elevation_query): Query<AspectElevationQuery>,
) -> axum::response::Result<impl IntoResponse> {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "image/png".parse().unwrap());
    let aspect_elevation =
        AspectElevation::try_from(aspect_elevation_query).map_err(map_eyre_error)?;
    let png_data = tokio::task::spawn_blocking(move || {
        generate_png(aspect_elevation).wrap_err("Error generating png")
    })
    .await
    .map_err(map_std_error)?
    .map_err(map_eyre_error)?;
    Ok((headers, png_data))
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use super::{generate_svg, Aspect, AspectElevation};

    #[test]
    fn test_generate_svg_empty() {
        let svg = generate_svg(AspectElevation::default());
        insta::assert_snapshot!(svg);
    }

    #[test]
    fn test_generate_svg_all_aspects() {
        let all_aspects: HashSet<Aspect> = Aspect::enumerate().into_iter().cloned().collect();
        let svg = generate_svg(AspectElevation {
            high_alpine: all_aspects.clone(),
            alpine: all_aspects.clone(),
            sub_alpine: all_aspects.clone(),
            ..AspectElevation::default()
        });
        insta::assert_snapshot!(svg);
    }

    #[test]
    fn test_generate_svg_alpine_n_w() {
        let svg = generate_svg(AspectElevation {
            high_alpine: HashSet::default(),
            alpine: vec![Aspect::N, Aspect::NW].into_iter().collect(),
            sub_alpine: HashSet::default(),
            ..AspectElevation::default()
        });
        insta::assert_snapshot!(svg);
    }

    #[test]
    fn test_generate_svg_text() {
        let svg = generate_svg(AspectElevation {
            high_alpine_text: Some("Test High Alpine".to_owned()),
            alpine_text: Some("Test Alpine".to_owned()),
            sub_alpine_text: Some("Test Sub Alpine".to_owned()),
            ..AspectElevation::default()
        });
        insta::assert_snapshot!(svg);
    }
}
