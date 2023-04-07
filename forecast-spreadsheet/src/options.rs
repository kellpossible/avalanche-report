use std::collections::HashMap;

use serde::Deserialize;

use crate::{serde::string, SheetCellPosition};

#[derive(Deserialize)]
pub struct Options {
    /// What version of the spreadsheet these options are for.
    #[serde(with = "string")]
    pub schema_version: crate::Version,
    pub template_version: Version,
    pub language: Language,
    pub area: Area,
    pub forecaster: Forecaster,
    pub time: Time,
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
pub struct Version {
    pub position: SheetCellPosition,
}

#[derive(Deserialize)]
pub struct Area {
    pub position: SheetCellPosition,
    /// A map from area name to area identifier.
    pub map: HashMap<String, String>,
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
