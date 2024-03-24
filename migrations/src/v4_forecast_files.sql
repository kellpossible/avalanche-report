CREATE TABLE IF NOT EXISTS forecast_files (
    google_drive_id TEXT NOT NULL PRIMARY KEY,
    last_modified NUMERIC NOT NULL,
    file_blob BLOB NOT NULL
);
