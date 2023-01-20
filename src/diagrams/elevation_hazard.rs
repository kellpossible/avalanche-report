use axum::{
    extract::Query,
    http::{header, HeaderMap},
    response::{Html, IntoResponse},
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ElevationBand {
    HighAlpine,
    Alpine,
    SubAlpine,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum HazardLevel {
    Low = 1,
    Medium = 2,
    Considerable = 3,
    High = 4,
    Extreme = 5,
}

const WHITE: &str = "#ffffffff";

impl HazardLevel {
    fn colour_hex(&self) -> &'static str {
        match self {
            HazardLevel::Low => "#00bb0aff",
            HazardLevel::Medium => "#fdff22ff",
            HazardLevel::Considerable => "#f88000ff",
            HazardLevel::High => "#f80000ff",
            HazardLevel::Extreme => "#00000000",
        }
    }
}

#[derive(Deserialize)]
pub struct ElevationHazard {
    band: ElevationBand,
    level: HazardLevel,
}

pub fn generate_svg(elevation_hazard: ElevationHazard) -> String {
    let high_alpine_colour = match elevation_hazard.band {
        ElevationBand::HighAlpine => elevation_hazard.level.colour_hex(),
        _ => WHITE,
    };
    let alpine_colour = match elevation_hazard.band {
        ElevationBand::Alpine => elevation_hazard.level.colour_hex(),
        _ => WHITE,
    };
    let sub_alpine_colour = match elevation_hazard.band {
        ElevationBand::SubAlpine => elevation_hazard.level.colour_hex(),
        _ => WHITE,
    };

    format!(
        r##"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!-- Created with Inkscape (http://www.inkscape.org/) -->

<svg
   width="400"
   height="400"
   viewBox="0 0 105.83333 105.83334"
   version="1.1"
   id="svg5"
   xmlns="http://www.w3.org/2000/svg"
   xmlns:svg="http://www.w3.org/2000/svg">
  <defs
     id="defs2" />
  <g
     id="layer1">
    <path
       style="fill:{high_alpine_colour};stroke:#000000;stroke-width:0.264583px;stroke-linecap:butt;stroke-linejoin:miter;stroke-opacity:1"
       d="M 54.569168,4.0210426 34.735194,39.481727 73.493156,39.781862 Z"
       id="high-alpine" />
    <path
       style="fill:{alpine_colour};stroke:#000000;stroke-width:0.264583px;stroke-linecap:butt;stroke-linejoin:miter;stroke-opacity:1"
       d="M 17.818713,68.375259 88.758537,68.188805 73.493156,39.781862 34.735194,39.481727 Z"
       id="alpine" />
    <path
       style="fill:{sub_alpine_colour};fill-opacity:1;stroke:none;stroke-width:0.980408;stroke-miterlimit:4;stroke-dasharray:none;stroke-opacity:1"
       d="M 2.2299132,96.35494 17.818713,68.375259 88.758536,68.188803 103.88618,96.473276 Z"
       id="sub-alpine" />
    <path
       style="fill:none;stroke:#000000;stroke-width:1.05833335;stroke-linecap:butt;stroke-linejoin:miter;stroke-miterlimit:4;stroke-dasharray:none;stroke-opacity:1"
       d="M 2.2299132,96.35494 54.569167,4.0210425 103.88618,96.473276 Z"
       id="path44" />
    <path
       style="fill:none;stroke:#000000;stroke-width:1.05833335;stroke-linecap:butt;stroke-linejoin:miter;stroke-miterlimit:4;stroke-dasharray:none;stroke-opacity:1"
       d="M 17.818713,68.375259 88.758536,68.188803"
       id="path1031" />
    <path
       style="fill:none;stroke:#000000;stroke-width:1.05833335;stroke-linecap:butt;stroke-linejoin:miter;stroke-miterlimit:4;stroke-dasharray:none;stroke-opacity:1"
       d="m 34.735195,39.481726 38.757959,0.300136"
       id="path1033" />
  </g>
</svg>
"##
    )
}

pub async fn svg_handler(elevation_hazard: Query<ElevationHazard>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "image/svg+xml".parse().unwrap());
    (headers, generate_svg(elevation_hazard.0))
}
