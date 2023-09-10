use std::{fmt::Display, io::Cursor, iter::repeat, num::ParseIntError, ops::Deref, str::FromStr};

pub mod options;
pub mod position;
mod serde;

use ::serde::{Deserialize, Serialize};
use calamine::{open_workbook_auto_from_rs, DataType, Reader, Sheets};
use eyre::{Context, ContextCompat};
use indexmap::{IndexMap, IndexSet};
use once_cell::sync::Lazy;
use options::{HazardRatingInput, Options};
use position::SheetCellPosition;
use time::{Date, Month, PrimitiveDateTime, Time};

static EXCEL_EPOCH: Lazy<PrimitiveDateTime> = Lazy::new(|| {
    Date::from_calendar_date(1899, Month::December, 30)
        .unwrap()
        .with_hms(0, 0, 0)
        .unwrap()
});

#[derive(Debug)]
pub enum ParseCellErrorKind {
    Calamine(calamine::Error),
    IncorrectDataType,
    CellMissing,
    SheetMissing,
    FromStr(Box<dyn std::error::Error + Send + Sync + 'static>),
}

pub struct ParseCellError {
    kind: ParseCellErrorKind,
    position: SheetCellPosition,
    value: Option<DataType>,
    context: Option<Box<dyn Fn() -> String + Send + Sync + 'static>>,
}

impl std::fmt::Debug for ParseCellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParseCellError")
            .field("kind", &self.kind)
            .field("position", &self.position)
            .field("value", &self.value)
            .field("context", &self.context.as_ref().map(|function| function()))
            .finish()
    }
}

impl ParseCellError {
    pub fn incorrect_data_type(position: SheetCellPosition, value: DataType) -> Self {
        Self {
            kind: ParseCellErrorKind::IncorrectDataType,
            position,
            value: Some(value),
            context: None,
        }
    }

    pub fn cell_missing(position: SheetCellPosition) -> Self {
        Self {
            kind: ParseCellErrorKind::CellMissing,
            position,
            value: None,
            context: None,
        }
    }

    pub fn sheet_missing(position: SheetCellPosition) -> Self {
        Self {
            kind: ParseCellErrorKind::SheetMissing,
            position,
            value: None,
            context: None,
        }
    }

    pub fn calamine(position: SheetCellPosition, error: calamine::Error) -> Self {
        Self {
            kind: ParseCellErrorKind::Calamine(error),
            position,
            value: None,
            context: None,
        }
    }

    pub fn from_str_error<E: std::error::Error + Send + Sync + 'static>(
        position: SheetCellPosition,
        value: DataType,
        error: E,
    ) -> Self {
        Self {
            kind: ParseCellErrorKind::FromStr(Box::new(error)),
            position,
            value: Some(value),
            context: None,
        }
    }
}

pub trait ParseCellWithContext {
    fn cell_wrap_err_with(self, context: Box<dyn Fn() -> String + Send + Sync + 'static>) -> Self;
}

impl ParseCellWithContext for ParseCellError {
    fn cell_wrap_err_with(
        mut self,
        context: Box<dyn Fn() -> String + Send + Sync + 'static>,
    ) -> Self {
        self.context = Some(context.into());
        self
    }
}

impl<T> ParseCellWithContext for std::result::Result<T, ParseCellError> {
    fn cell_wrap_err_with(self, context: Box<dyn Fn() -> String + Send + Sync + 'static>) -> Self {
        match self {
            Ok(_) => self,
            Err(error) => Err(error.cell_wrap_err_with(context)),
        }
    }
}

impl std::error::Error for ParseCellError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ParseCellErrorKind::Calamine(error) => Some(error),
            _ => None,
        }
    }
}

impl Display for ParseCellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error while parsing cell at position {0} with value {1:?}: ",
            self.position, self.value
        )?;

        match &self.kind {
            ParseCellErrorKind::IncorrectDataType => write!(f, "Incorrect data type"),
            ParseCellErrorKind::CellMissing => write!(
                f,
                "The cell at position {0} does not exist",
                self.position.position
            ),
            ParseCellErrorKind::SheetMissing => {
                write!(f, "The sheet {0} does not exist", self.position.sheet)
            }
            ParseCellErrorKind::Calamine(error) => write!(f, "{error}"),
            ParseCellErrorKind::FromStr(error) => write!(f, "{error}"),
        }?;

        if let Some(context) = &self.context {
            write!(f, "{}", context())?;
        }

        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseVersionError {
    #[error("Incorrect format for version {0:?}, expected e.g. 1.0.4")]
    IncorrectFormat(String),
    #[error("Unable to parse the version number as an integer")]
    ParseInt(#[from] ParseIntError),
}

impl FromStr for Version {
    type Err = ParseVersionError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let mut split = s.split('.');
        let major = split
            .next()
            .ok_or_else(|| ParseVersionError::IncorrectFormat(s.to_owned()))?
            .parse()?;
        let minor = split
            .next()
            .ok_or_else(|| ParseVersionError::IncorrectFormat(s.to_owned()))?
            .parse()?;
        let patch = split
            .next()
            .ok_or_else(|| ParseVersionError::IncorrectFormat(s.to_owned()))?
            .parse()?;

        if split.next().is_some() {
            return Err(ParseVersionError::IncorrectFormat(s.to_owned()));
        }

        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct ElevationRange {
    pub upper: Option<i64>,
    pub lower: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[repr(transparent)]
#[serde(transparent)]
pub struct AreaId(String);

impl Deref for AreaId {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
#[serde(transparent)]
pub struct ElevationBandId(String);

impl From<String> for ElevationBandId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ElevationBandId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl Deref for ElevationBandId {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HazardRatingKind {
    Overall,
    ElevationSpecific(ElevationBandId),
}

impl<'de> Deserialize<'de> for HazardRatingKind {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "overall" => Self::Overall,
            _ => Self::ElevationSpecific(ElevationBandId(value)),
        })
    }
}

impl Serialize for HazardRatingKind {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: ::serde::Serializer,
    {
        match self {
            HazardRatingKind::Overall => serializer.serialize_str("overall"),
            HazardRatingKind::ElevationSpecific(e) => serializer.serialize_str(&*e),
        }
    }
}

impl Display for HazardRatingKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HazardRatingKind::ElevationSpecific(band_id) => f.write_str(&*band_id),
            HazardRatingKind::Overall => f.write_str("overall"),
        }
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum HazardRatingValue {
    Low = 1,
    Moderate = 2,
    Considerable = 3,
    High = 4,
    Extreme = 5,
}

impl FromStr for HazardRatingValue {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
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

#[derive(Debug, thiserror::Error)]
pub enum ParseAspectError {
    #[error("Invalid value {0}")]
    InvalidValue(String),
}

impl FromStr for Aspect {
    type Err = ParseAspectError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "N" => Ok(Aspect::N),
            "NE" => Ok(Aspect::NE),
            "E" => Ok(Aspect::E),
            "SE" => Ok(Aspect::SE),
            "S" => Ok(Aspect::S),
            "SW" => Ok(Aspect::SW),
            "W" => Ok(Aspect::W),
            "NW" => Ok(Aspect::NW),
            _ => Err(ParseAspectError::InvalidValue(s.to_string())),
        }
    }
}

fn parse_aspects(input: &str) -> std::result::Result<IndexSet<Aspect>, ParseAspectError> {
    if input.is_empty() {
        return Ok(IndexSet::with_capacity(0));
    }
    input
        .split(',')
        .map(|s| s.trim())
        .map(Aspect::from_str)
        .collect()
}

#[derive(Debug, Serialize)]
pub struct AspectElevation {
    pub aspects: IndexSet<Aspect>,
}

#[derive(Debug, Serialize)]
pub struct AvalancheProblem {
    pub kind: ProblemKind,
    pub aspect_elevation: IndexMap<ElevationBandId, AspectElevation>,
    pub confidence: Option<Confidence>,
    pub trend: Option<Trend>,
    pub size: Option<Size>,
    pub distribution: Option<Distribution>,
    pub time_of_day: Option<TimeOfDay>,
    pub sensitivity: Option<Sensitivity>,
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProblemKind {
    LooseDry,
    LooseWet,
    StormSlab,
    WindSlab,
    WetSlab,
    PersistentSlab,
    DeepSlab,
    Cornice,
    Glide,
}

impl FromStr for ProblemKind {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Distribution {
    Isolated,
    Specific,
    Widespread,
}

impl FromStr for Distribution {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trend {
    Improving,
    NoChange,
    Deteriorating,
}

impl FromStr for Trend {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    Low,
    Moderate,
    High,
}

impl FromStr for Confidence {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sensitivity {
    Unreactive,
    Stubborn,
    Reactive,
    Touchy,
}

impl FromStr for Sensitivity {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimeOfDay {
    AllDay,
    Morning,
    Afternoon,
}

impl FromStr for TimeOfDay {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

// Scope of `serde` module conflicts with serde_repr
mod size {
    use std::str::FromStr;

    use serde_repr::{Deserialize_repr, Serialize_repr};
    #[derive(Serialize_repr, Deserialize_repr, PartialEq, Debug)]
    #[repr(u8)]
    pub enum Size {
        One = 1,
        Two = 2,
        Three = 3,
        Four = 4,
        Five = 5,
    }

    impl FromStr for Size {
        type Err = serde_json::Error;

        fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
            serde_json::from_str(s)
        }
    }

    impl TryFrom<u8> for Size {
        type Error = eyre::Error;

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Ok(match value {
                1 => Self::One,
                2 => Self::Two,
                3 => Self::Three,
                4 => Self::Four,
                5 => Self::Five,
                _ => return Err(eyre::eyre!("cannot parse size from value {value}")),
            })
        }
    }
}
pub use size::Size;

#[derive(Debug, Serialize)]
pub struct HazardRating {
    pub value: Option<HazardRatingValue>,
    pub trend: Option<Trend>,
    pub confidence: Option<Confidence>,
}

#[derive(Debug, Serialize)]
pub struct Forecast {
    pub template_version: Version,
    pub language: unic_langid::LanguageIdentifier,
    pub area: AreaId,
    pub forecaster: Forecaster,
    pub time: PrimitiveDateTime,
    pub recent_observations: Option<String>,
    pub forecast_changes: Option<String>,
    pub weather_forecast: Option<String>,
    pub valid_for: time::Duration,
    pub description: Option<String>,
    pub hazard_ratings: IndexMap<HazardRatingKind, HazardRating>,
    pub avalanche_problems: Vec<AvalancheProblem>,
    pub elevation_bands: IndexMap<ElevationBandId, ElevationRange>,
}

#[derive(Debug, Serialize)]
pub struct Forecaster {
    pub name: String,
    pub organisation: Option<String>,
}

fn get_cell_value<RS>(
    sheets: &mut Sheets<RS>,
    position: &SheetCellPosition,
) -> std::result::Result<DataType, ParseCellError>
where
    RS: std::io::Read + std::io::Seek,
{
    let sheet = sheets
        .worksheet_range(&position.sheet)
        .ok_or_else(|| ParseCellError::sheet_missing(position.clone()))?
        .map_err(|error| ParseCellError::calamine(position.clone(), error))?;

    Ok(sheet
        .get_value(position.position.into())
        .ok_or_else(|| ParseCellError::cell_missing(position.clone()))?
        .clone())
}

fn get_cell_value_bool<RS>(
    sheets: &mut Sheets<RS>,
    position: &SheetCellPosition,
) -> std::result::Result<bool, ParseCellError>
where
    RS: std::io::Read + std::io::Seek,
{
    let value = get_cell_value(sheets, position)?;
    value
        .get_bool()
        .ok_or_else(|| ParseCellError::incorrect_data_type(position.clone(), value))
}

fn get_cell_value_string<T, RS>(
    sheets: &mut Sheets<RS>,
    position: &SheetCellPosition,
) -> Result<Option<T>, ParseCellError>
where
    RS: std::io::Read + std::io::Seek,
    T: FromStr,
    <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    let value = get_cell_value(sheets, position)?;
    if value.is_empty() {
        return Ok(None);
    }
    let value_str = value
        .get_string()
        .ok_or_else(|| ParseCellError::incorrect_data_type(position.clone(), value.clone()))?;

    value_str
        .parse()
        .map(Some)
        .map_err(|error| ParseCellError::from_str_error(position.clone(), value, error))
}

/// Convert an Excel decimal day into a [`Time`].
fn decimal_day_to_time(time_day: f64) -> std::result::Result<Time, time::error::ComponentRange> {
    let hour = (time_day * 24.0).floor() as u64;
    let minute = (time_day * 24.0 * 60.0).floor() as u64 - (hour * 60);
    let second = (time_day * 24.0 * 60.0 * 60.0).floor() as u64 - (hour * 60 * 60) - (minute * 60);
    let millisecond = (time_day * 24.0 * 60.0 * 60.0 * 1000.0).round() as u64
        - (hour * 60 * 60 * 1000)
        - (minute * 60 * 1000)
        - (second * 1000);
    Time::from_hms_milli(hour as u8, minute as u8, second as u8, millisecond as u16)
}

fn get_cell_value_time<RS>(
    sheets: &mut Sheets<RS>,
    position: &SheetCellPosition,
) -> std::result::Result<Time, ParseCellError>
where
    RS: std::io::Read + std::io::Seek,
{
    let value = get_cell_value(sheets, position)?;

    match value {
        DataType::Float(f) | DataType::DateTime(f) => {
            let time_day = f - f.floor();
            decimal_day_to_time(time_day).map_err(|error| {
                ParseCellError::from_str_error(position.clone(), value.clone(), error)
            })
        }
        _ => Err(ParseCellError::incorrect_data_type(position.clone(), value)),
    }
}

fn get_cell_value_datetime<RS>(
    sheets: &mut Sheets<RS>,
    position: &SheetCellPosition,
) -> std::result::Result<PrimitiveDateTime, ParseCellError>
where
    RS: std::io::Read + std::io::Seek,
{
    let value = get_cell_value(sheets, position)?;

    match value {
        DataType::Float(f) | DataType::DateTime(f) => {
            let julian_day = EXCEL_EPOCH.to_julian_day() + (f.floor() as i32);
            let time_day = f - f.floor();
            let time = decimal_day_to_time(time_day).map_err(|error| {
                ParseCellError::from_str_error(position.clone(), value.clone(), error)
            })?;
            let date = Date::from_julian_day(julian_day).map_err(|error| {
                ParseCellError::from_str_error(position.clone(), value.clone(), error)
            })?;

            Ok(PrimitiveDateTime::new(date, time))
        }
        _ => Err(ParseCellError::incorrect_data_type(position.clone(), value)),
    }
}

fn required_value_missing<V: std::fmt::Display>(
    value: V,
    position: SheetCellPosition,
) -> eyre::Error {
    eyre::eyre!("Required value {value} missing from position {position}")
}

fn unable_to_map_value<V: std::fmt::Display, M: std::fmt::Display>(
    map: M,
    value: V,
) -> eyre::Error {
    eyre::eyre!("Unable to use map {map} to find a valid variant equal to value {value}")
}

pub fn parse_excel_spreadsheet(
    spreadsheet_bytes: &[u8],
    options: &Options,
) -> eyre::Result<Forecast> {
    let cursor = Cursor::new(spreadsheet_bytes);
    // open_workbook_auto_from_rs(data)
    let mut sheets: Sheets<_> = open_workbook_auto_from_rs(cursor)?;

    let template_version: Version = get_cell_value_string(&mut sheets, &options.template_version)?
        .ok_or_else(|| {
            required_value_missing("template_version", options.template_version.clone())
        })?;

    let language_name: String = get_cell_value_string(&mut sheets, &options.language.position)?
        .ok_or_else(|| required_value_missing("language", options.language.position.clone()))?;

    let language: unic_langid::LanguageIdentifier = options
        .language
        .map
        .get(&language_name)
        .ok_or_else(|| eyre::eyre!("unknown language {language_name}"))?
        .clone();

    let area_name: String = get_cell_value_string(&mut sheets, &options.area.position)?
        .ok_or_else(|| required_value_missing("area", options.area.position.clone()))?;
    let area = options
        .area
        .map
        .get(&area_name)
        .ok_or_else(|| eyre::eyre!("unknown area {area_name}"))?
        .to_owned();

    let forecaster = {
        let name =
            get_cell_value_string(&mut sheets, &options.forecaster.name)?.ok_or_else(|| {
                required_value_missing("forecaster.name", options.forecaster.name.clone())
            })?;
        let organisation = get_cell_value_string(&mut sheets, &options.forecaster.organisation)?;
        Forecaster { name, organisation }
    };

    let time = {
        match &options.time {
            options::Time::DateAndTime {
                date: date_position,
                time: time_position,
            } => {
                let date = get_cell_value_datetime(&mut sheets, &date_position)?;
                let time = get_cell_value_time(&mut sheets, &time_position)?;
                PrimitiveDateTime::new(date.date(), time)
            }
        }
    };

    let recent_observations: Option<String> = Option::transpose(
        options
            .recent_observations
            .as_ref()
            .map(|recent_observations| get_cell_value_string(&mut sheets, recent_observations)),
    )?
    .flatten();

    let forecast_changes: Option<String> = Option::transpose(
        options
            .forecast_changes
            .as_ref()
            .map(|position| get_cell_value_string(&mut sheets, position)),
    )?
    .flatten();

    let weather_forecast: Option<String> = Option::transpose(
        options
            .weather_forecast
            .as_ref()
            .map(|position| get_cell_value_string(&mut sheets, position)),
    )?
    .flatten();

    let valid_for = {
        let value = get_cell_value(&mut sheets, &options.valid_for)?;
        let days: f64 = match value {
            DataType::Int(i) => i as f64,
            DataType::Float(f) => f,
            DataType::Empty => {
                return Err(required_value_missing(
                    "valid_for",
                    options.valid_for.clone(),
                ))
            }
            _ => {
                return Err(
                    ParseCellError::incorrect_data_type(options.valid_for.clone(), value).into(),
                )
            }
        };

        let ms = days * 24.0 * 60.0 * 60.0 * 1000.0;

        time::Duration::milliseconds(ms as i64)
    };

    let description: Option<String> = Option::transpose(
        options
            .description
            .as_ref()
            .map(|position| get_cell_value_string(&mut sheets, position)),
    )?
    .flatten();

    let hazard_ratings = options
        .hazard_ratings
        .inputs
        .iter()
        .map(|(kind, input)| {
            let kind = kind.clone();
            match extract_hazard_rating(&kind, input, &mut sheets, options) {
                Ok(hazard_rating) => Ok((kind, hazard_rating)),
                Err(error) => Err(error)
                    .wrap_err_with(|| format!("error extracting hazard rating {kind}: {input:?}")),
            }
        })
        .collect::<eyre::Result<_>>()?;

    let avalanche_problems: Vec<AvalancheProblem> = options
        .avalanche_problems
        .iter()
        .enumerate()
        .map(|(i, problem)| {
            extract_avalanch_problem(problem, options, &mut sheets)
                .wrap_err_with(|| format!("Avalanche problem {i}"))
        })
        .filter_map(std::result::Result::transpose)
        .collect::<eyre::Result<_>>()?;

    let mut elevation_band_boundaries: Vec<i64> = get_cell_value(
        &mut sheets,
        &options.area.elevation_band_boundaries.position,
    )
    .context("Error getting elevation band boundaries value")?
    .to_string()
    .replace('m', "")
    .split(",")
    .map(|altitude| {
        altitude
            .trim()
            .parse()
            .wrap_err_with(|| format!("Error parsing elevation band boundary {altitude}"))
    })
    .collect::<eyre::Result<_>>()?;
    if options.area.elevation_band_boundaries.reverse {
        elevation_band_boundaries.reverse();
    }

    let elevation_band_windows: Vec<Option<i64>> = std::iter::once(None)
        .chain(elevation_band_boundaries.into_iter().map(Some))
        .chain(std::iter::once(None))
        .collect();
    let elevation_bands = elevation_band_windows
        .windows(2)
        .enumerate()
        .map(|(i, window)| {
            let elevation_band_id = options
                .elevation_bands
                .get_index(i)
                .wrap_err_with(|| format!("Cannot get elevation band from schema for index {i}"))?;

            let range = ElevationRange {
                lower: window[0],
                upper: window[1],
            };

            Ok((elevation_band_id.clone(), range))
        })
        .collect::<eyre::Result<_>>()?;

    Ok(Forecast {
        language,
        template_version,
        area,
        forecaster,
        time,
        recent_observations,
        forecast_changes,
        weather_forecast,
        valid_for,
        description,
        hazard_ratings,
        avalanche_problems,
        elevation_bands,
    })
}

fn extract_avalanch_problem<RS>(
    problem: &options::AvalancheProblem,
    options: &Options,
    sheets: &mut Sheets<RS>,
) -> eyre::Result<Option<AvalancheProblem>>
where
    RS: std::io::Read + std::io::Seek,
{
    let enabled_cell = problem.root.clone() + problem.enabled;
    let enabled = get_cell_value_bool(sheets, &enabled_cell)?;

    // Skip this problem if it is disabled.
    if !enabled {
        return Ok(None);
    }

    let kind_cell = problem.root.clone() + problem.kind;
    let value: String = get_cell_value_string(sheets, &kind_cell)
        .wrap_err_with(Box::new(move || format!("kind")))?
        .ok_or_else(|| required_value_missing("avalanche_problem.kind", kind_cell.clone()))?;

    let kind = options
        .terms
        .avalanche_problem_kind
        .get(&value)
        .cloned()
        .ok_or_else(|| unable_to_map_value("terms.avalanche_problem_kind", value))?;

    let aspect_elevation = problem
        .aspect_elevation
        .iter()
        .zip(repeat(problem))
        .map(|((elevation_band, aspect_elevation), problem)| {
            let enabled_cell = problem.root.clone() + aspect_elevation.enabled;
            let enabled = get_cell_value_bool(sheets, &enabled_cell)?;

            if !enabled {
                return Ok(None);
            }

            let aspects_cell = problem.root.clone() + aspect_elevation.aspects;
            let value: String = match get_cell_value_string(sheets, &aspects_cell)? {
                Some(value) => value,
                None => String::new(),
            };
            let aspects = parse_aspects(&value).map_err(|error| {
                ParseCellError::from_str_error(aspects_cell.clone(), DataType::String(value), error)
            })?;

            if aspects.is_empty() {
                return Ok(None);
            }

            Ok(Some((elevation_band.clone(), AspectElevation { aspects })))
        })
        .filter_map(std::result::Result::transpose)
        .collect::<eyre::Result<_>>()?;

    let trend: Option<Trend> = Option::transpose(problem.trend.map(|relative| {
        let cell = problem.root.clone() + relative;

        Option::transpose(get_cell_value_string(sheets, &cell).context("trend")?.map(
            |value: String| {
                options
                    .terms
                    .trend
                    .get(&value)
                    .cloned()
                    .ok_or_else(|| unable_to_map_value("terms.trend", value))
            },
        ))
    }))?
    .flatten();

    let confidence: Option<Confidence> = Option::transpose(problem.confidence.map(|relative| {
        let cell = problem.root.clone() + relative;

        Option::transpose(
            get_cell_value_string(sheets, &cell)
                .context("confidence")?
                .map(|value: String| {
                    options
                        .terms
                        .confidence
                        .get(&value)
                        .cloned()
                        .ok_or_else(|| unable_to_map_value("terms.confidence", value))
                }),
        )
    }))?
    .flatten();

    let sensitivity: Option<Sensitivity> =
        Option::transpose(problem.sensitivity.map(|relative| {
            let cell = problem.root.clone() + relative;

            Option::transpose(
                get_cell_value_string(sheets, &cell)
                    .context("sensitivity")?
                    .map(|value: String| {
                        options
                            .terms
                            .sensitivity
                            .get(&value)
                            .cloned()
                            .ok_or_else(|| unable_to_map_value("terms.sensitivity", value))
                    }),
            )
        }))?
        .flatten();

    let time_of_day: Option<TimeOfDay> = Option::transpose(problem.time_of_day.map(|relative| {
        let cell = problem.root.clone() + relative;

        Option::transpose(
            get_cell_value_string(sheets, &cell)
                .context("time_of_day")?
                .map(|value: String| {
                    options
                        .terms
                        .time_of_day
                        .get(&value)
                        .cloned()
                        .ok_or_else(|| unable_to_map_value("terms.time_of_day", value))
                }),
        )
    }))?
    .flatten();

    let distribution: Option<Distribution> =
        Option::transpose(problem.distribution.map(|relative| {
            let cell = problem.root.clone() + relative;

            Option::transpose(
                get_cell_value_string(sheets, &cell)
                    .context("distribution")?
                    .map(|value: String| {
                        options
                            .terms
                            .distribution
                            .get(&value)
                            .cloned()
                            .ok_or_else(|| unable_to_map_value("terms.distribution", value))
                    }),
            )
        }))?
        .flatten();

    let size: Option<Size> = Option::transpose(problem.size.map(|relative| {
        let cell = problem.root.clone() + relative;

        Ok(Some(match get_cell_value(sheets, &cell)? {
            DataType::Int(n) => Size::try_from(u8::try_from(n)?)?,
            DataType::Float(f) => Size::try_from(u8::try_from(f as i64)?)?,
            DataType::Empty => return Ok(None),
            unexpected => return Err(eyre::eyre!("unexpected data type {unexpected:?}")),
        }))
    }))
    .context("size")?
    .flatten();

    Result::Ok(Some(AvalancheProblem {
        kind,
        aspect_elevation,
        trend,
        confidence,
        size,
        distribution,
        sensitivity,
        time_of_day,
    }))
}

fn extract_hazard_rating<RS>(
    kind: &HazardRatingKind,
    input: &HazardRatingInput,
    sheets: &mut Sheets<RS>,
    options: &Options,
) -> eyre::Result<HazardRating>
where
    RS: std::io::Read + std::io::Seek,
{
    if let HazardRatingKind::ElevationSpecific(elevation_band) = kind {
        if !options.elevation_bands.contains(elevation_band) {
            return Err(unable_to_map_value(
                "elevation_bands",
                (**elevation_band).clone(),
            ));
        }
    }
    let value_cell = input.root.clone() + input.value;
    let value: Option<HazardRatingValue> = Option::transpose(
        get_cell_value_string(sheets, &value_cell)?.map(|value: String| {
            options
                .terms
                .hazard_rating
                .get(&value)
                .cloned()
                .ok_or_else(|| unable_to_map_value("terms.hazard_rating", value))
        }),
    )?;

    let trend: Option<Trend> = Option::transpose(input.trend.map(|relative| {
        let cell = input.root.clone() + relative;

        Option::transpose(get_cell_value_string(sheets, &cell).context("trend")?.map(
            |value: String| {
                options
                    .terms
                    .trend
                    .get(&value)
                    .cloned()
                    .ok_or_else(|| unable_to_map_value("terms.trend", value))
            },
        ))
    }))?
    .flatten();

    let confidence: Option<Confidence> = Option::transpose(input.confidence.map(|relative| {
        let cell = input.root.clone() + relative;

        Option::transpose(
            get_cell_value_string(sheets, &cell)
                .context("confidence")?
                .map(|value: String| {
                    options
                        .terms
                        .confidence
                        .get(&value)
                        .cloned()
                        .ok_or_else(|| unable_to_map_value("terms.confidence", value))
                }),
        )
    }))?
    .flatten();

    let rating = HazardRating {
        value,
        trend,
        confidence,
    };

    Ok(rating)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    const CRATE_DIR: &'static str = env!("CARGO_MANIFEST_DIR");

    use crate::options::Options;

    use super::parse_excel_spreadsheet;
    #[test]
    fn test_parse_excel_spreadsheet() {
        let fixtures = Path::new(CRATE_DIR).join("fixtures");
        let spreadsheet_bytes =
            std::fs::read(fixtures.join("forecasts/Gudauri_2023_02_07T19 00_LS.xlsx")).unwrap();
        let options: Options = serde_json::from_str(
            &std::fs::read_to_string(fixtures.join("options/options.0.3.0.json")).unwrap(),
        )
        .unwrap();
        let forecast = parse_excel_spreadsheet(&spreadsheet_bytes, &options).unwrap();

        insta::assert_json_snapshot!(&forecast);
    }
}
