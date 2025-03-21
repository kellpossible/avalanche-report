use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    str::FromStr,
    sync::Arc,
};

use axum::{
    extract,
    http::{header, HeaderMap},
    response::IntoResponse,
    Extension,
};
use eyre::Context;
use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed_fl::fl;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use resvg::{
    tiny_skia,
    usvg::{self, PostProcessingSteps},
};
use serde::{Deserialize, Serialize};

use crate::{
    error::{map_eyre_error, map_std_error},
    i18n::I18nLoader,
};

use super::FONT_DB;

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
            Aspect::N => "N",
            Aspect::NE => "NE",
            Aspect::E => "E",
            Aspect::SE => "SE",
            Aspect::S => "S",
            Aspect::SW => "SW",
            Aspect::W => "W",
            Aspect::NW => "NW",
        }
    }
    fn svg_id(&self) -> &'static str {
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

impl Display for Aspect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

impl FromStr for Aspect {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_uppercase().as_str() {
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

#[derive(Debug)]
pub struct Colour(String);

impl Colour {
    pub fn try_from_hex(hex: String) -> eyre::Result<Self> {
        if HEX_COLOUR_RE.is_match(&hex) {
            Ok(Self(hex))
        } else {
            Err(eyre::eyre!("Invalid hex colour string: {hex:?}"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub struct AspectElevation {
    pub high_alpine: HashSet<Aspect>,
    pub high_alpine_text: Option<String>,
    pub alpine: HashSet<Aspect>,
    pub alpine_text: Option<String>,
    pub sub_alpine: HashSet<Aspect>,
    pub sub_alpine_text: Option<String>,
    pub colour: Colour,
}

impl Default for AspectElevation {
    fn default() -> Self {
        Self {
            high_alpine: Default::default(),
            high_alpine_text: Default::default(),
            alpine: Default::default(),
            alpine_text: Default::default(),
            sub_alpine: Default::default(),
            sub_alpine_text: Default::default(),
            colour: Colour(DEFAULT_FILLED_COLOUR.to_owned()),
        }
    }
}

impl AspectElevation {
    pub fn into_query(self) -> Query {
        self.into()
    }
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

fn iter_to_comma_separated(aspects: impl IntoIterator<Item = Aspect>) -> String {
    aspects
        .into_iter()
        .map(|aspect| aspect.id())
        .collect::<Vec<_>>()
        .join(",")
}

impl TryFrom<Query> for AspectElevation {
    type Error = eyre::Error;

    fn try_from(query: Query) -> Result<Self, Self::Error> {
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
        let colour = query
            .colour
            .map(Colour::try_from_hex)
            .unwrap_or(Ok(Colour(DEFAULT_FILLED_COLOUR.to_owned())))
            .wrap_err("Error parsing colour query parameter")?;
        Ok(Self {
            high_alpine,
            high_alpine_text: query.high_alpine_text,
            alpine,
            alpine_text: query.alpine_text,
            sub_alpine,
            sub_alpine_text: query.sub_alpine_text,
            colour,
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Query {
    high_alpine: Option<String>,
    high_alpine_text: Option<String>,
    alpine: Option<String>,
    alpine_text: Option<String>,
    sub_alpine: Option<String>,
    sub_alpine_text: Option<String>,
    colour: Option<String>,
}

impl From<AspectElevation> for Query {
    fn from(value: AspectElevation) -> Self {
        Self {
            high_alpine: Some(iter_to_comma_separated(value.high_alpine)),
            high_alpine_text: value.high_alpine_text,
            alpine: Some(iter_to_comma_separated(value.alpine)),
            alpine_text: value.alpine_text,
            sub_alpine: Some(iter_to_comma_separated(value.sub_alpine)),
            sub_alpine_text: value.sub_alpine_text,
            colour: Some(value.colour.as_str().to_owned()),
        }
    }
}

const SVG_TEMPLATE: &str = include_str!("./aspect_elevation.svg");
const DEFAULT_FILLED_COLOUR: &str = "#276fdc";
static PATH_ID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"<path\s*style="(?P<fill>fill:(?P<colour>#ffffff);)([^/])*id="(?P<id>[a-z\-]+)"\s*[/]>"#,
    )
    .expect("Unable to compile svg path id regex")
});

static HEX_COLOUR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^#([A-Fa-f0-9]{6})$"#).expect("Unable to compile hex colour RE"));

static TEXT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"<text([^/])*id="(?P<id>[a-z\-]+)"(.|\s)*?<tspan(.|\s)*?>(?P<text>.*?)<[/]tspan>(.|\s)*?<[/]text>"#
    )
    .expect("Unable to compile svg text regex")
});

fn generate_svg(aspect_elevation: AspectElevation, i18n: Arc<FluentLanguageLoader>) -> String {
    let high_alpine_ids = aspect_elevation.high_alpine.iter().map(|aspect| {
        let id = aspect.svg_id();
        format!("high-alpine-{id}")
    });
    let alpine_ids = aspect_elevation.alpine.iter().map(|aspect| {
        let id = aspect.svg_id();
        format!("alpine-{id}")
    });
    let sub_alpine_ids = aspect_elevation.sub_alpine.iter().map(|aspect| {
        let id = aspect.svg_id();
        format!("sub-alpine-{id}")
    });

    let colour_map: HashMap<String, &str> = high_alpine_ids
        .chain(alpine_ids)
        .chain(sub_alpine_ids)
        .map(|id| (id, aspect_elevation.colour.as_str()))
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
                    let high_alpine_text = aspect_elevation
                        .high_alpine_text
                        .to_owned()
                        .unwrap_or_else(|| fl!(&*i18n, "elevation-band-high-alpine").to_string());
                    captured_string.replace(original_text, &high_alpine_text)
                }
                "alpine-text" => {
                    let alpine_text = aspect_elevation
                        .alpine_text
                        .to_owned()
                        .unwrap_or_else(|| fl!(&*i18n, "elevation-band-alpine").to_string());
                    captured_string.replace(original_text, &alpine_text)
                }
                "sub-alpine-text" => {
                    let sub_alpine_text = aspect_elevation
                        .sub_alpine_text
                        .to_owned()
                        .unwrap_or_else(|| fl!(&*i18n, "elevation-band-sub-alpine").to_string());
                    captured_string.replace(original_text, &sub_alpine_text)
                }
                _ => captured_string.to_string(),
            }
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
    let aspect_elevation = AspectElevation::try_from(query).map_err(map_eyre_error)?;
    Ok((headers, generate_svg(aspect_elevation, i18n)))
}

fn generate_png(
    aspect_elevation: AspectElevation,
    i18n: Arc<FluentLanguageLoader>,
) -> eyre::Result<Vec<u8>> {
    let svg = generate_svg(aspect_elevation, i18n);
    let options = usvg::Options::default();
    let mut tree = usvg::Tree::from_str(&svg, &options)?;
    tree.postprocess(
        PostProcessingSteps {
            convert_text_into_paths: true,
        },
        &FONT_DB,
    );
    let pixmap_size = tree.size.to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
        .ok_or_else(|| eyre::eyre!("Unable to create pixmap"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(eyre::Error::from)
}

pub async fn png_handler(
    extract::Query(aspect_elevation_query): extract::Query<Query>,
    Extension(i18n): Extension<I18nLoader>,
) -> axum::response::Result<impl IntoResponse> {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "image/png".parse().unwrap());
    let aspect_elevation =
        AspectElevation::try_from(aspect_elevation_query).map_err(map_eyre_error)?;
    let png_data = tokio::task::spawn_blocking(move || {
        generate_png(aspect_elevation, i18n).wrap_err("Error generating png")
    })
    .await
    .map_err(map_std_error)?
    .map_err(map_eyre_error)?;
    Ok((headers, png_data))
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use crate::{diagrams::aspect_elevation::Colour, i18n::{self, load_available_languages, I18nLoader}};

    use super::{generate_svg, Aspect, AspectElevation};

    use once_cell::sync::Lazy;
    use unic_langid::LanguageIdentifier;

    static LOADER: Lazy<I18nLoader> = Lazy::new(|| {
        let (loader, _) = i18n::initialize(&crate::options::I18n::default()).unwrap();
        load_available_languages(&loader, &["en-UK".parse::<LanguageIdentifier>().unwrap()])
            .unwrap();
        loader
    });

    #[test]
    fn test_generate_svg_empty() {
        let svg = generate_svg(AspectElevation::default(), LOADER.clone());
        insta::assert_snapshot!(svg);
    }

    #[test]
    fn test_generate_svg_all_aspects() {
        let all_aspects: HashSet<Aspect> = Aspect::enumerate().into_iter().cloned().collect();
        let svg = generate_svg(
            AspectElevation {
                high_alpine: all_aspects.clone(),
                alpine: all_aspects.clone(),
                sub_alpine: all_aspects.clone(),
                ..AspectElevation::default()
            },
            LOADER.clone(),
        );
        insta::assert_snapshot!(svg);
    }

    #[test]
    fn test_generate_svg_all_aspects_colour() {
        let all_aspects: HashSet<Aspect> = Aspect::enumerate().into_iter().cloned().collect();
        let svg = generate_svg(
            AspectElevation {
                high_alpine: all_aspects.clone(),
                alpine: all_aspects.clone(),
                sub_alpine: all_aspects.clone(),
                colour: Colour::try_from_hex("#ff5500".to_owned()).unwrap(),
                ..AspectElevation::default()
            },
            LOADER.clone(),
        );
        insta::assert_snapshot!(svg);
    }

    #[test]
    fn test_generate_svg_alpine_n_w() {
        let svg = generate_svg(
            AspectElevation {
                high_alpine: HashSet::default(),
                alpine: vec![Aspect::N, Aspect::NW].into_iter().collect(),
                sub_alpine: HashSet::default(),
                ..AspectElevation::default()
            },
            LOADER.clone(),
        );
        insta::assert_snapshot!(svg);
    }

    #[test]
    fn test_generate_svg_text() {
        let svg = generate_svg(
            AspectElevation {
                high_alpine_text: Some("Test High Alpine".to_owned()),
                alpine_text: Some("Test Alpine".to_owned()),
                sub_alpine_text: Some("Test Sub Alpine".to_owned()),
                ..AspectElevation::default()
            },
            LOADER.clone(),
        );
        insta::assert_snapshot!(svg);
    }
}
