use bytes::Bytes;
use http::HeaderValue;
use reqwest::{header::CONTENT_TYPE, Url};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFilesResult {
    pub files: Vec<ListFileMetadata>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListFilesQuery<'a> {
    q: &'a str,
    key: &'a str,
}

/// As per
/// [stackoverflow](https://stackoverflow.com/questions/18116152/how-do-i-get-a-file-list-for-a-google-drive-public-hosted-folder),
/// obtain a list of files on a google drive.
pub async fn list_files(
    folder_id: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<Vec<ListFileMetadata>> {
    let q = format!("'{folder_id}' in parents and trashed = false");
    let query = ListFilesQuery {
        q: &q,
        key: api_key.expose_secret(),
    };
    let query_string = serde_urlencoded::to_string(query)?;
    let url: Url = format!("https://www.googleapis.com/drive/v3/files?{query_string}").parse()?;
    let response = client.get(url).send().await?;
    let result: ListFilesResult = response.json().await?;
    Ok(result.files)
}

pub struct File {
    response: reqwest::Response,
}

impl File {
    pub fn content_type(&self) -> Option<HeaderValue> {
        self.response.headers().get(CONTENT_TYPE).cloned()
    }
    pub fn bytes_stream(self) -> impl futures::stream::Stream<Item = reqwest::Result<Bytes>> {
        self.response.bytes_stream()
    }

    pub async fn bytes(self) -> reqwest::Result<Bytes> {
        self.response.bytes().await
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetFileQuery<'a> {
    alt: Option<&'a str>,
    key: &'a str,
}

pub async fn get_file(
    file_id: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<File> {
    let query = GetFileQuery {
        alt: Some("media"),
        key: api_key.expose_secret(),
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

pub async fn get_file_metadata(
    file_id: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<FileMetadata> {
    let query = GetFileQuery {
        alt: None,
        key: api_key.expose_secret(),
    };
    let query_string = serde_urlencoded::to_string(query)?;
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
