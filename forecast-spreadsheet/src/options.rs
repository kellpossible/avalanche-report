use std::collections::HashMap;

use indexmap::{IndexMap, IndexSet};
use serde::Deserialize;

use crate::{
    position::CellPosition, serde::string, AreaId, Confidence, Distribution, ElevationBandId,
    HazardRatingKind, HazardRatingValue, ProblemKind, Sensitivity, SheetCellPosition, TimeOfDay,
    Trend, Version,
};

#[derive(Deserialize)]
pub struct Options {
    /// What version of the spreadsheet this schema applies to.
    #[serde(with = "string")]
    pub schema_version: Version,
    pub template_version: SheetCellPosition,
    pub language: Language,
    pub area: Area,
    pub area_definitions: IndexMap<AreaId, AreaDefinition>,
    pub forecaster: Forecaster,
    pub time: Time,
    pub recent_observations: Option<SheetCellPosition>,
    pub forecast_changes: Option<SheetCellPosition>,
    pub weather_forecast: Option<SheetCellPosition>,
    pub valid_for: SheetCellPosition,
    pub description: Option<SheetCellPosition>,
    pub hazard_ratings: HazardRatings,
    pub avalanche_problems: Vec<AvalancheProblem>,
    /// Set of elevation band ids, that needs to match the order and number of
    /// elevation boundaries in [`Area::elevation_band_boundaries`].
    pub elevation_bands: IndexSet<ElevationBandId>,
    pub terms: Terms,
}

mod timezone_from_string {
    pub fn deserialize<'de, D>(deserializer: D) -> Result<&'static time_tz::Tz, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = &'static time_tz::Tz;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(
                    "Expecting a valid IANA timezone name string e.g. \"Africa/Abidjan\"",
                )
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                time_tz::timezones::get_by_name(v)
                    .ok_or_else(|| {
                        E::custom(format!("Unable to find timezone {v} in IANA database"))
                    })
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[derive(Deserialize)]
pub struct AreaDefinition {
    #[serde(with = "timezone_from_string")]
    pub time_zone: &'static time_tz::Tz,
}

#[derive(Deserialize)]
pub struct Terms {
    pub confidence: HashMap<String, Confidence>,
    pub hazard_rating: HashMap<String, HazardRatingValue>,
    pub trend: HashMap<String, Trend>,
    pub avalanche_problem_kind: HashMap<String, ProblemKind>,
    pub distribution: HashMap<String, Distribution>,
    pub time_of_day: HashMap<String, TimeOfDay>,
    pub sensitivity: HashMap<String, Sensitivity>,
}

/// The affected aspects for a given elevation for an [`AvalancheProblem`]
#[derive(Deserialize)]
pub struct AspectElevation {
    /// Whether this particular element is enabled.
    pub enabled: CellPosition,
    pub aspects: CellPosition,
}

#[derive(Deserialize)]
pub struct AvalancheProblem {
    pub root: SheetCellPosition,
    /// Whether this avalanche problem is specified/enabled.
    pub enabled: CellPosition,
    pub kind: CellPosition,
    pub aspect_elevation: IndexMap<ElevationBandId, AspectElevation>,
    pub confidence: Option<CellPosition>,
    pub sensitivity: Option<CellPosition>,
    pub size: Option<CellPosition>,
    pub distribution: Option<CellPosition>,
    pub time_of_day: Option<CellPosition>,
    pub trend: Option<CellPosition>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Time {
    DateAndTime {
        date: SheetCellPosition,
        time: SheetCellPosition,
    },
}

#[derive(Deserialize)]
pub struct HazardRatings {
    pub inputs: IndexMap<HazardRatingKind, HazardRatingInput>,
}

#[derive(Deserialize)]
pub struct ElevationRange {
    pub upper: Option<i64>,
    pub lower: Option<i64>,
}

#[derive(Deserialize, Debug)]
pub struct HazardRatingInput {
    /// Root position of the hazard rating block.
    pub root: SheetCellPosition,
    /// Position of the hazard rating value cell relative to `root`.
    pub value: CellPosition,
    /// Position of the trend cell relative to `root`.
    pub trend: Option<CellPosition>,
    /// Position of the confidence cell relative to `root`.
    pub confidence: Option<CellPosition>,
}

/// Comma separated list of elevation band boundaries. The length of this should be
/// `elevation_bands.len() - 1` for example "2000m,4000m"
#[derive(Deserialize)]
pub struct ElevationBandBoundaries {
    pub position: SheetCellPosition,
    pub reverse: bool,
}

#[derive(Deserialize)]
pub struct Area {
    pub position: SheetCellPosition,
    /// A map from area name to area identifier.
    pub map: HashMap<String, AreaId>,
    pub elevation_band_boundaries: ElevationBandBoundaries,
}

#[derive(Deserialize)]
pub struct Language {
    pub position: SheetCellPosition,
    /// A map from language name (in the spreadsheet) to language identifier.
    pub map: HashMap<String, unic_langid::LanguageIdentifier>,
}

#[derive(Deserialize)]
pub struct Forecaster {
    pub name: SheetCellPosition,
    pub organisation: SheetCellPosition,
}
