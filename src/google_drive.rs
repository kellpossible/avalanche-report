use bytes::Bytes;
use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tracing::instrument;

/// Truncated version of <https://developers.google.com/drive/api/reference/rest/v3/files#File>
/// that appears to be returned while listing files with [list_files].
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFileMetadata {
    /// The MIME type of the file.
    ///
    /// Google Drive attempts to automatically detect an appropriate value from uploaded content,
    /// if no value is provided. The value cannot be changed unless a new revision is uploaded.
    ///
    /// If a file is created with a Google Doc MIME type, the uploaded content is imported, if
    /// possible. The supported import formats are published in the About resource.
    pub mime_type: String,
    /// The ID of the file.
    pub id: String,
    /// The name of the file. This is not necessarily unique within a folder. Note that for
    /// immutable items such as the top level folders of shared drives, My Drive root folder, and
    /// Application Data folder the name is constant.
    pub name: String,
    /// The last time the file was modified by anyone.
    #[serde(with = "time::serde::rfc3339")]
    pub modified_time: time::OffsetDateTime,
}

impl ListFileMetadata {
    pub fn is_google_sheet(&self) -> bool {
        self.mime_type == "application/vnd.google-apps.spreadsheet"
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum ListFilesResult {
    Files(Vec<ListFileMetadata>),
    Error(GoogleDriveError),
}

impl From<ListFilesResult> for Result<Vec<ListFileMetadata>, GoogleDriveError> {
    fn from(value: ListFilesResult) -> Self {
        match value {
            ListFilesResult::Files(files) => Ok(files),
            ListFilesResult::Error(error) => Err(error),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GoogleDriveError {
    pub code: u16,
    pub errors: Vec<serde_json::Value>,
    pub message: String,
}

impl std::fmt::Display for GoogleDriveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "code: {}, message: {}", self.code, self.message)
    }
}

impl std::error::Error for GoogleDriveError {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListFilesQuery<'a> {
    q: &'a str,
    key: &'a str,
    fields: &'a str,
}

/// As per
/// [stackoverflow](https://stackoverflow.com/questions/18116152/how-do-i-get-a-file-list-for-a-google-drive-public-hosted-folder),
/// obtain a list of files on a google drive.
#[instrument(skip_all, fields(folder_id))]
pub async fn list_files(
    folder_id: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<Vec<ListFileMetadata>> {
    let q = format!("'{folder_id}' in parents and trashed = false");
    let query = ListFilesQuery {
        q: &q,
        key: api_key.expose_secret(),
        fields: "files(mimeType, id, name, modifiedTime)",
    };
    let query_string = serde_urlencoded::to_string(query)?;
    let url: Url = format!("https://www.googleapis.com/drive/v3/files?{query_string}").parse()?;
    let response = client.get(url).send().await?;
    let files = Result::from(response.json::<ListFilesResult>().await?)?;
    Ok(files)
}

pub fn get_file_in_list<'a>(
    file_name: &str,
    file_list: &'a [ListFileMetadata],
) -> Option<&'a ListFileMetadata> {
    match file_list
        .iter()
        .find(|file_metadata| file_metadata.name == file_name)
    {
        Some(file_metadata) => Some(file_metadata),
        None => return None,
    }
}

pub struct File {
    response: reqwest::Response,
}

impl File {
    pub async fn bytes(self) -> reqwest::Result<Bytes> {
        self.response.bytes().await
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
struct GetFileQuery<'a> {
    alt: Option<&'a str>,
    fields: Option<&'a str>,
    key: Option<&'a str>,
}

#[instrument(skip_all, fields(file_id))]
pub async fn get_file(
    file_id: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<File> {
    let query = GetFileQuery {
        alt: Some("media"),
        key: Some(api_key.expose_secret()),
        ..GetFileQuery::default()
    };
    let query_string = serde_urlencoded::to_string(query)?;
    let url: Url =
        format!("https://www.googleapis.com/drive/v3/files/{file_id}?{query_string}").parse()?;
    let response = client.get(url).send().await?;
    Ok(File { response })
}

/// <https://developers.google.com/drive/api/reference/rest/v3/files#File>
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    /// The MIME type of the file.
    ///
    /// Google Drive attempts to automatically detect an appropriate value from uploaded content,
    /// if no value is provided. The value cannot be changed unless a new revision is uploaded.
    ///
    /// If a file is created with a Google Doc MIME type, the uploaded content is imported, if
    /// possible. The supported import formats are published in the About resource.
    pub mime_type: String,
    /// The ID of the file.
    pub id: String,
    /// The name of the file. This is not necessarily unique within a folder. Note that for
    /// immutable items such as the top level folders of shared drives, My Drive root folder, and
    /// Application Data folder the name is constant.
    pub name: String,
    /// The last time the file was modified by anyone.
    #[serde(with = "time::serde::rfc3339")]
    pub modified_time: time::OffsetDateTime,
}

#[instrument(skip_all, fields(file_id))]
pub async fn get_file_metadata(
    file_id: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<FileMetadata> {
    let query = GetFileQuery {
        key: Some(api_key.expose_secret()),
        fields: Some("*"),
        ..GetFileQuery::default()
    };
    let query_string = serde_urlencoded::to_string(query)?;
    dbg!(&query_string);
    let url: Url =
        format!("https://www.googleapis.com/drive/v3/files/{file_id}?{query_string}").parse()?;
    let response = client.get(url).send().await?;
    let metadata: FileMetadata = response.json().await?;
    Ok(metadata)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportFileQuery<'a> {
    mime_type: &'a str,
    key: &'a str,
}

/// Mime type from <https://developers.google.com/drive/api/guides/ref-export-formats>
#[instrument(skip_all, fields(file_id))]
pub async fn export_file(
    file_id: &str,
    mime_type: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<File> {
    let query = ExportFileQuery {
        mime_type,
        key: api_key.expose_secret(),
    };
    let query_string = serde_urlencoded::to_string(query)?;
    let url: Url =
        format!("https://www.googleapis.com/drive/v3/files/{file_id}/export?{query_string}")
            .parse()?;
    let response = client.get(url).send().await?;
    Ok(File { response })
}
