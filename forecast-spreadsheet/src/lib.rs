use std::io::Cursor;

use calamine::{open_workbook_auto_from_rs, DataType, Range, Reader, Sheets};

#[derive(Debug)]
pub struct Position {
    pub row: u32,
    pub column: u32,
}

impl From<(u32, u32)> for Position {
    fn from(value: (u32, u32)) -> Self {
        Self {
            row: value.0,
            column: value.1,
        }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({0}, {1})", self.row, self.column)
    }
}

#[derive(Debug)]
pub struct SheetPosition {
    pub sheet: String,
    pub position: Position,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The spreadsheet does not contain the sheet {0}")]
    SpreadsheetMissingSheet(String),
    #[error("The spreadsheet does not contain value at {0:?}")]
    SpreadsheetMissingValue(SheetPosition),
    #[error(transparent)]
    Calamine(#[from] calamine::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct Forecast {
    template_version: String,
    language: String,
    area: String,
    time: time::OffsetDateTime,
}

pub fn parse_excel_spreadsheet(spreadsheet_bytes: &[u8]) -> Result<()> {
    let cursor = Cursor::new(spreadsheet_bytes);
    // open_workbook_auto_from_rs(data)
    let mut sheets: Sheets<_> = open_workbook_auto_from_rs(cursor)?;
    let worksheet: Range<_> = sheets
        .worksheet_range("Form")
        .ok_or_else(|| Error::SpreadsheetMissingSheet("Form".to_owned()))??;
    let language: &DataType = worksheet.get_value((1, 1)).ok_or_else(|| {
        Error::SpreadsheetMissingValue(SheetPosition {
            sheet: "Form".to_owned(),
            position: (1, 1).into(),
        })
    })?;
    dbg!(language);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_excel_spreadsheet;
    #[test]
    fn test_parse_excel_spreadsheet() {
        let spreadsheet_bytes =
            std::fs::read("fixtures/forecasts/Gudauri_2023_02_07T19 00_LS.xlsx").unwrap();
        parse_excel_spreadsheet(&spreadsheet_bytes).unwrap();
    }
}
