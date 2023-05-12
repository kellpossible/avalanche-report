use rusqlite::Row;
use sea_query::SimpleExpr;

use crate::{database::DATETIME_FORMAT, types};

#[derive(Clone)]
#[sea_query::enum_def]
pub struct ForecastFiles {
    pub google_drive_id: String,
    pub last_modified: types::Time,
    pub file_blob: Vec<u8>,
}

impl ForecastFiles {
    pub const COLUMNS: [ForecastFilesIden; 3] = [
        ForecastFilesIden::GoogleDriveId,
        ForecastFilesIden::LastModified,
        ForecastFilesIden::FileBlob,
    ];
    pub const TABLE: ForecastFilesIden = ForecastFilesIden::Table;

    pub fn values(self) -> [SimpleExpr; 3] {
        let last_modified = self.last_modified_value();
        [
            self.google_drive_id.into(),
            last_modified,
            self.file_blob.into(),
        ]
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
        }
    }
}
impl TryFrom<&Row<'_>> for ForecastFiles {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> Result<Self, Self::Error> {
        let google_drive_id = row.get(ForecastFilesIden::GoogleDriveId.as_ref())?;
        let last_modified = row.get(ForecastFilesIden::LastModified.as_ref())?;
        let file_blob = row.get(ForecastFilesIden::FileBlob.as_ref())?;

        Ok(ForecastFiles {
            google_drive_id,
            last_modified,
            file_blob,
        })
    }
}
