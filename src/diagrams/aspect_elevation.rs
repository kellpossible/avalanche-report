use std::{collections::HashSet, str::FromStr};

use axum::{
    extract::Query,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
};
use eyre::Context;
use serde::Deserialize;

#[derive(Debug, PartialEq, Eq, Hash)]
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
        .map(|value| value.trim().parse::<Aspect>())
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

pub async fn svg_handler(
    Query(aspect_elevation_query): Query<AspectElevationQuery>,
) -> axum::response::Result<impl IntoResponse> {
    let mut headers = HeaderMap::new();
    // headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    headers.insert(header::CONTENT_TYPE, "text/plain".parse().unwrap());
    let aspect_elevation = AspectElevation::try_from(aspect_elevation_query).map_err(|error| {
        tracing::error!("{:?}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((headers, format!("{aspect_elevation:?}")))
}
