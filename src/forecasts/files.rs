use std::str::FromStr;

use rusqlite::{types::Type, Row};
use sea_query::SimpleExpr;

use crate::{database::DATETIME_FORMAT, types};

#[derive(Clone)]
#[sea_query::enum_def]
pub struct ForecastFiles {
    pub google_drive_id: String,
    pub last_modified: types::Time,
    pub file_blob: Vec<u8>,
    pub parsed_forecast: Option<forecast_spreadsheet::Forecast>,
    pub schema_version: Option<forecast_spreadsheet::Version>,
}

impl ForecastFiles {
    pub const COLUMNS: [ForecastFilesIden; 5] = [
        ForecastFilesIden::GoogleDriveId,
        ForecastFilesIden::LastModified,
        ForecastFilesIden::FileBlob,
        ForecastFilesIden::ParsedForecast,
        ForecastFilesIden::SchemaVersion,
    ];
    pub const TABLE: ForecastFilesIden = ForecastFilesIden::Table;

    pub fn values(self) -> eyre::Result<[SimpleExpr; 5]> {
        let last_modified = self.last_modified_value();
        Ok([
            self.google_drive_id.into(),
            last_modified,
            self.file_blob.into(),
            self.parsed_forecast
                .map(|f| serde_json::to_string(&f))
                .transpose()?
                .into(),
            self.schema_version.map(|v| v.to_string()).into(),
        ])
    }

    pub fn last_modified_value(&self) -> SimpleExpr {
        self.last_modified
            .format(&DATETIME_FORMAT)
            .expect("Error formatting time")
            .into()
    }
}

impl AsRef<str> for ForecastFilesIden {
    fn as_ref(&self) -> &str {
        match self {
            Self::Table => "forecast_files",
            Self::GoogleDriveId => "google_drive_id",
            Self::LastModified => "last_modified",
            Self::FileBlob => "file_blob",
            Self::ParsedForecast => "parsed_forecast",
            Self::SchemaVersion => "schema_version",
        }
    }
}
impl TryFrom<&Row<'_>> for ForecastFiles {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        let google_drive_id = row.get(ForecastFilesIden::GoogleDriveId.as_ref())?;
        let last_modified = row.get(ForecastFilesIden::LastModified.as_ref())?;
        let file_blob = row.get(ForecastFilesIden::FileBlob.as_ref())?;
        let parsed_forecast: Option<serde_json::Value> =
            row.get(ForecastFilesIden::ParsedForecast.as_ref())?;
        let schema_version: Option<String> = row.get(ForecastFilesIden::SchemaVersion.as_ref())?;

        Ok(ForecastFiles {
            google_drive_id,
            last_modified,
            file_blob,
            parsed_forecast: parsed_forecast
                .map(|f| {
                    serde_json::from_value(f).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(e))
                    })
                })
                .transpose()?,
            schema_version: schema_version
                .map(|v| {
                    forecast_spreadsheet::Version::from_str(&v).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(5, Type::Text, Box::new(e))
                    })
                })
                .transpose()?,
        })
    }
}
