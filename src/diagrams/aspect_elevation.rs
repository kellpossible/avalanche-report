use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    str::FromStr,
};

use axum::{
    extract::Query,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
};
use eyre::Context;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use serde::Deserialize;

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

#[derive(Debug)]
pub struct AspectElevation {
    high_alpine: HashSet<Aspect>,
    alpine: HashSet<Aspect>,
    sub_alpine: HashSet<Aspect>,
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

    fn try_from(value: AspectElevationQuery) -> Result<Self, Self::Error> {
        let high_alpine = value
            .high_alpine
            .map(comma_separated_to_vec)
            .unwrap_or(Ok(HashSet::default()))
            .wrap_err("Error deserializing high_alpine")?;
        let alpine = value
            .alpine
            .map(comma_separated_to_vec)
            .unwrap_or(Ok(HashSet::default()))
            .wrap_err("Error deserializing alpine")?;
        let sub_alpine = value
            .sub_alpine
            .map(comma_separated_to_vec)
            .unwrap_or(Ok(HashSet::default()))
            .wrap_err("Error deserializing sub_alpine")?;
        Ok(Self {
            high_alpine,
            alpine,
            sub_alpine,
        })
    }
}

#[derive(Deserialize, Debug)]
pub struct AspectElevationQuery {
    high_alpine: Option<String>,
    alpine: Option<String>,
    sub_alpine: Option<String>,
}

const SVG_TEMPLATE: &str = include_str!("./aspect_elevation.svg");
const FILLED_COLOUR: &str = "#276fdcff";
static PATH_ID_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"<path\s*style="(?P<fill>fill:(?P<colour>#ffffff);)([^/])*id="(?P<id>[a-z\-]+)"\s*[/]>"#,
    )
    .expect("Unable to compile svg path id regex")
});

fn generate_svg(aspect_elevation: AspectElevation) -> Cow<'static, str> {
    let high_alpine_ids = aspect_elevation.high_alpine.into_iter().map(|aspect| {
        let id = aspect.id();
        format!("high-alpine-{id}")
    });
    let alpine_ids = aspect_elevation.alpine.into_iter().map(|aspect| {
        let id = aspect.id();
        format!("alpine-{id}")
    });
    let sub_alpine_ids = aspect_elevation.sub_alpine.into_iter().map(|aspect| {
        let id = aspect.id();
        format!("sub-alpine-{id}")
    });

    let colour_map: HashMap<String, &str> = high_alpine_ids
        .chain(alpine_ids)
        .chain(sub_alpine_ids)
        .map(|id| (id, FILLED_COLOUR))
        .collect();

    PATH_ID_RE.replace_all(SVG_TEMPLATE, |captures: &Captures| {
        let id = captures.name("id").unwrap().as_str();
        let captured_string = captures.get(0).unwrap().as_str();
        if let Some(colour) = colour_map.get(id) {
            let fill_group_string = captures.name("fill").unwrap().as_str();
            captured_string.replace(fill_group_string, &format!("fill:{colour};"))
        } else {
            captured_string.to_string()
        }
    })
}

pub async fn svg_handler(
    Query(aspect_elevation_query): Query<AspectElevationQuery>,
) -> axum::response::Result<impl IntoResponse> {
    let mut headers = HeaderMap::new();
    // headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    let aspect_elevation = AspectElevation::try_from(aspect_elevation_query).map_err(|error| {
        tracing::error!("{:?}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((headers, generate_svg(aspect_elevation)))
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use super::{generate_svg, Aspect, AspectElevation};

    #[test]
    fn test_generate_svg_empty() {
        let svg = generate_svg(AspectElevation {
            high_alpine: vec![].into_iter().collect(),
            alpine: vec![].into_iter().collect(),
            sub_alpine: vec![].into_iter().collect(),
        });
        insta::assert_snapshot!(svg);
    }

    #[test]
    fn test_generate_svg_all_aspects() {
        let all_aspects: HashSet<Aspect> = Aspect::enumerate().into_iter().cloned().collect();
        let svg = generate_svg(AspectElevation {
            high_alpine: all_aspects.clone(),
            alpine: all_aspects.clone(),
            sub_alpine: all_aspects.clone(),
        });
        insta::assert_snapshot!(svg);
    }

    #[test]
    fn test_generate_svg_alpine_n_w() {
        let svg = generate_svg(AspectElevation {
            high_alpine: HashSet::default(),
            alpine: vec![Aspect::N, Aspect::NW].into_iter().collect(),
            sub_alpine: HashSet::default(),
        });
        insta::assert_snapshot!(svg);
    }
}
