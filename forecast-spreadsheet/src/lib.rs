use std::{fmt::Display, io::Cursor, num::ParseIntError, str::FromStr};

pub mod options;
pub mod position;
mod serde;

use ::serde::Serialize;
use calamine::{open_workbook_auto_from_rs, DataType, Reader, Sheets};
use once_cell::sync::Lazy;
use options::Options;
use position::SheetCellPosition;
use time::{Date, Month, PrimitiveDateTime, Time};

static EXCEL_EPOCH: Lazy<PrimitiveDateTime> = Lazy::new(|| {
    Date::from_calendar_date(1899, Month::December, 30)
        .unwrap()
        .with_hms(0, 0, 0)
        .unwrap()
});

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Calamine(#[from] calamine::Error),
    #[error("Unable to parse version")]
    UnableToParseVersion(#[from] ParseVersionError),
    #[error("Unable to parse a cell")]
    UnableToParseCell(#[from] ParseCellError),
    #[error("Unknown language {0}")]
    UnknownLanguage(String),
    #[error("Unknown area {0}")]
    UnknownArea(String),
    #[error("Invalid language identifier")]
    InvalidLanguageIdentifier(#[from] unic_langid::LanguageIdentifierError),
    #[error("Required value {0} is missing at position {1}")]
    RequiredValueMissing(String, SheetCellPosition),
}

impl Error {
    pub fn required_value_missing<S: Into<String>>(value: S, position: SheetCellPosition) -> Self {
        Self::RequiredValueMissing(value.into(), position)
    }
}

#[derive(Debug)]
pub enum ParseCellErrorKind {
    Calamine(calamine::Error),
    IncorrectDataType,
    CellMissing,
    SheetMissing,
    FromStr(Box<dyn std::error::Error + Send + Sync + 'static>),
}

#[derive(Debug)]
pub struct ParseCellError {
    kind: ParseCellErrorKind,
    position: SheetCellPosition,
    value: Option<DataType>,
}

impl ParseCellError {
    pub fn incorrect_data_type(position: SheetCellPosition, value: DataType) -> Self {
        Self {
            kind: ParseCellErrorKind::IncorrectDataType,
            position,
            value: Some(value),
        }
    }

    pub fn cell_missing(position: SheetCellPosition) -> Self {
        Self {
            kind: ParseCellErrorKind::CellMissing,
            position,
            value: None,
        }
    }

    pub fn sheet_missing(position: SheetCellPosition) -> Self {
        Self {
            kind: ParseCellErrorKind::SheetMissing,
            position,
            value: None,
        }
    }

    pub fn calamine(position: SheetCellPosition, error: calamine::Error) -> Self {
        Self {
            kind: ParseCellErrorKind::Calamine(error),
            position,
            value: None,
        }
    }

    pub fn from_std_error<E: std::error::Error + Send + Sync + 'static>(
        position: SheetCellPosition,
        value: DataType,
        error: E,
    ) -> Self {
        Self {
            kind: ParseCellErrorKind::FromStr(Box::new(error)),
            position,
            value: Some(value),
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
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

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
pub struct Forecast {
    template_version: Version,
    language: unic_langid::LanguageIdentifier,
    area: String,
    forecaster: Forecaster,
    time: PrimitiveDateTime,
    recent_observations: Option<String>,
    forecast_changes: Option<String>,
    weather_forecast: Option<String>,
}

#[derive(Debug, Serialize)]
struct Forecaster {
    name: String,
    organisation: Option<String>,
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

fn get_cell_value_from_str<T, RS>(
    sheets: &mut Sheets<RS>,
    position: &SheetCellPosition,
) -> std::result::Result<Option<T>, ParseCellError>
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
        .map_err(|error| ParseCellError::from_std_error(position.clone(), value, error))
}

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
                ParseCellError::from_std_error(position.clone(), value.clone(), error)
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
                ParseCellError::from_std_error(position.clone(), value.clone(), error)
            })?;
            let date = Date::from_julian_day(julian_day).map_err(|error| {
                ParseCellError::from_std_error(position.clone(), value.clone(), error)
            })?;

            Ok(PrimitiveDateTime::new(date, time))
        }
        _ => Err(ParseCellError::incorrect_data_type(position.clone(), value)),
    }
}

pub fn parse_excel_spreadsheet(spreadsheet_bytes: &[u8], options: &Options) -> Result<Forecast> {
    let cursor = Cursor::new(spreadsheet_bytes);
    // open_workbook_auto_from_rs(data)
    let mut sheets: Sheets<_> = open_workbook_auto_from_rs(cursor)?;

    let template_version: Version =
        get_cell_value_from_str(&mut sheets, &options.template_version)?.ok_or_else(|| {
            Error::required_value_missing("template_version", options.template_version.clone())
        })?;

    let language_name: String = get_cell_value_from_str(&mut sheets, &options.language.position)?
        .ok_or_else(|| {
        Error::required_value_missing("language", options.language.position.clone())
    })?;

    let language: unic_langid::LanguageIdentifier = options
        .language
        .map
        .get(&language_name)
        .ok_or_else(|| Error::UnknownLanguage(language_name))?
        .clone();

    let area_name: String = get_cell_value_from_str(&mut sheets, &options.area.position)?
        .ok_or_else(|| Error::required_value_missing("area", options.area.position.clone()))?;
    let area = options
        .area
        .map
        .get(&area_name)
        .ok_or_else(|| Error::UnknownArea(area_name))?
        .to_owned();

    let forecaster = {
        let name =
            get_cell_value_from_str(&mut sheets, &options.forecaster.name)?.ok_or_else(|| {
                Error::required_value_missing("forecaster.name", options.forecaster.name.clone())
            })?;
        let organisation = get_cell_value_from_str(&mut sheets, &options.forecaster.organisation)?;
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
            .map(|recent_observations| get_cell_value_from_str(&mut sheets, recent_observations)),
    )?
    .flatten();
    
    let forecast_changes: Option<String> = Option::transpose(
        options
            .forecast_changes
            .as_ref()
            .map(|forecast_changes| get_cell_value_from_str(&mut sheets, forecast_changes)),
    )?
    .flatten();
    
    let weather_forecast: Option<String> = Option::transpose(
        options
            .weather_forecast
            .as_ref()
            .map(|weather_forecast| get_cell_value_from_str(&mut sheets, weather_forecast)),
    )?
    .flatten();

    Ok(Forecast {
        language,
        template_version,
        area,
        forecaster,
        time,
        recent_observations,
        forecast_changes,
        weather_forecast,
    })
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
