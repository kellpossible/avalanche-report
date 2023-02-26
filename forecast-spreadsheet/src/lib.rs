use std::{fmt::Display, io::Cursor, num::ParseIntError, str::FromStr};

use ::serde::Deserialize;
use calamine::{open_workbook_auto_from_rs, DataType, Reader, Sheets};
use options::Options;
use time::OffsetDateTime;

pub mod options;
mod serde;

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CellPosition {
    pub column: u32,
    pub row: u32,
}

impl Into<(u32, u32)> for CellPosition {
    fn into(self) -> (u32, u32) {
        (self.row, self.column)
    }
}

impl From<(u32, u32)> for CellPosition {
    fn from(value: (u32, u32)) -> Self {
        Self {
            column: value.1,
            row: value.0,
        }
    }
}

impl std::fmt::Display for CellPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({0}, {1})", self.row, self.column)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SheetCellPosition {
    pub sheet: String,
    pub position: CellPosition,
}

impl Display for SheetCellPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{0}!{1}", self.sheet, self.position)
    }
}

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

    pub fn from_str_error<E: std::error::Error + Send + Sync + 'static>(
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Forecast {
    template_version: Version,
    language: unic_langid::LanguageIdentifier,
    area: String,
    // time: time::OffsetDateTime,
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
) -> std::result::Result<T, ParseCellError>
where
    RS: std::io::Read + std::io::Seek,
    T: FromStr,
    <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    let value = get_cell_value(sheets, position)?;
    let value_str = value
        .get_string()
        .ok_or_else(|| ParseCellError::incorrect_data_type(position.clone(), value.clone()))?;

    value_str
        .parse()
        .map_err(|error| ParseCellError::from_str_error(position.clone(), value, error))
}

fn get_cell_value_time<RS>(
    sheets: &Sheets<RS>,
    position: &SheetCellPosition,
) -> std::result::Result<OffsetDateTime, ParseCellError>
where
    RS: std::io::Read + std::io::Seek,
{
    todo!();
}

pub fn parse_excel_spreadsheet(spreadsheet_bytes: &[u8], options: &Options) -> Result<Forecast> {
    let cursor = Cursor::new(spreadsheet_bytes);
    // open_workbook_auto_from_rs(data)
    let mut sheets: Sheets<_> = open_workbook_auto_from_rs(cursor)?;

    let template_version: Version =
        get_cell_value_from_str(&mut sheets, &options.template_version.position)?;

    let language_name: String = get_cell_value_from_str(&mut sheets, &options.language.position)?;
    let language: unic_langid::LanguageIdentifier = options
        .language
        .map
        .get(&language_name)
        .ok_or_else(|| Error::UnknownLanguage(language_name))?
        .clone();

    let area_name: String = get_cell_value_from_str(&mut sheets, &options.area.position)?;
    let area = options
        .area
        .map
        .get(&area_name)
        .ok_or_else(|| Error::UnknownArea(area_name))?
        .to_owned();

    Ok(Forecast {
        language,
        template_version,
        area,
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
        dbg!(CRATE_DIR);
        let fixtures = Path::new(CRATE_DIR).join("fixtures");
        let spreadsheet_bytes =
            std::fs::read(fixtures.join("forecasts/Gudauri_2023_02_07T19 00_LS.xlsx")).unwrap();
        let options: Options = serde_json::from_str(
            &std::fs::read_to_string(fixtures.join("options/options.0.3.0.json")).unwrap(),
        )
        .unwrap();
        dbg!(parse_excel_spreadsheet(&spreadsheet_bytes, &options).unwrap());
    }
}
