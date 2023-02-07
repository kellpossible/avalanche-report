use bytes::Bytes;
use futures::TryStreamExt;
use http::HeaderValue;
use reqwest::{header::CONTENT_TYPE, Url};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    pub mime_type: String,
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFilesResult {
    pub files: Vec<FileMetadata>,
}

/// As per
/// [stackoverflow](https://stackoverflow.com/questions/18116152/how-do-i-get-a-file-list-for-a-google-drive-public-hosted-folder),
/// obtain a list of files on a google drive.
pub async fn list_files(
    folder_id: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<Vec<FileMetadata>> {
    let api_key = api_key.expose_secret();
    let url: Url = format!(
        "https://www.googleapis.com/drive/v3/files?q='{folder_id}'+in+parents&key={api_key}"
    )
    .parse()?;
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
    pub fn bytes_stream(self) -> impl futures::stream::Stream<Item = eyre::Result<Bytes>> {
        self.response.bytes_stream().map_err(eyre::Error::from)
    }
}

pub async fn get_file(
    file_id: &str,
    api_key: &SecretString,
    client: &reqwest::Client,
) -> eyre::Result<File> {
    let api_key = api_key.expose_secret();
    let url: Url =
        format!("https://www.googleapis.com/drive/v3/files/{file_id}?alt=media&key={api_key}")
            .parse()?;
    let response = client.get(url).send().await?;
    Ok(File { response })
}
