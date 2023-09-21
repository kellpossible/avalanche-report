use std::time::Duration;

use base64::Engine;
use eyre::{bail, Context, ContextCompat};
use http::{HeaderValue, StatusCode};
use humansize::format_size;
use md5::{Digest, Md5};
use rusty_s3::{Credentials, S3Action, UrlStyle};
use secrecy::{ExposeSecret, SecretString};
use time::OffsetDateTime;
use tracing::Instrument;

use crate::options;

use super::{Database, DB_FILE_NAME};

#[derive(Debug)]
struct BackupInfo {
    size: u64,
    entity_tag: String,
    version_id: Option<String>,
    expiration: Option<String>,
}

async fn perform_backup(config: &Config) -> eyre::Result<BackupInfo> {
    tracing::info!("Starting backup to s3...");
    let options::Backup {
        s3_endpoint,
        s3_bucket_name,
        s3_bucket_region,
        aws_access_key_id,
        ..
    } = &config.backup;
    let client = &config.client;
    let database = &config.database;

    let bucket = rusty_s3::Bucket::new(
        s3_endpoint.clone(),
        UrlStyle::VirtualHost,
        s3_bucket_name,
        s3_bucket_region,
    )?;

    let credentials = Credentials::new(
        aws_access_key_id,
        &*config.aws_secret_access_key.expose_secret(),
    );

    let head_bucket = bucket.head_bucket(Some(&credentials));
    let response = client
        .head(head_bucket.sign(Duration::from_secs(60 * 60)))
        .send()
        .await?;
    if response.status() == StatusCode::NOT_FOUND {
        bail!("Unable to perform backup, the bucket {s3_bucket_name} does not exist in the region {s3_bucket_region}")
    }

    let instance = database
        .get()
        .await
        .wrap_err("Error obtaining database instance from pool")?;
    let backup_dir = tokio::task::spawn_blocking(|| {
        tempfile::tempdir().wrap_err("Error creating temporary directory")
    })
    .await??;
    let backup_file = backup_dir.path().join(DB_FILE_NAME);
    let backup_file_query = backup_file.clone();
    instance
        .interact::<_, eyre::Result<()>>(move |conn: &mut rusqlite::Connection| {
            conn.execute("VACUUM main INTO ?1", [backup_file_query.to_str()])
                .wrap_err("Error performing VACUUM query")?;
            Ok(())
        })
        .await
        .wrap_err("Error performing database interaction")??;

    let mut put_object = bucket.put_object(Some(&credentials), DB_FILE_NAME);
    let headers = put_object.headers_mut();
    // headers.insert("Content-Type", "application/vnd.sqlite3");
    //
    let meta = tokio::fs::metadata(&backup_file).await?;
    let content_length = meta.len().to_string();
    headers.insert("content-length", &content_length);
    let backup_file_md5 = backup_file.clone();
    let md5sum = tokio::task::spawn_blocking::<_, eyre::Result<String>>(|| {
        let mut file = std::fs::File::open(backup_file_md5)?;
        let mut hasher = Md5::new();
        std::io::copy(&mut file, &mut hasher)?;
        let hash = hasher.finalize();
        let engine = base64::engine::general_purpose::STANDARD;
        Ok(engine.encode(&hash))
    })
    .await??;
    headers.insert("content-md5", &md5sum);

    let url = put_object.sign(Duration::from_secs(5 * 60 * 60));
    let file = tokio::fs::File::open(backup_file).await?;

    let request = client
        .put(url)
        .header("content-length", content_length)
        .header("content-md5", &md5sum);

    let response = request.body(file).send().await?;
    let status = response.status();
    if !status.is_success() {
        let url = response.url().clone();
        let text = response.text().await.unwrap_or_default();
        bail!("Error status code ({status}) for url ({url}). {text}")
    }

    let headers = response.headers();

    let info = BackupInfo {
        size: meta.len(),
        entity_tag: headers
            .get("ETag")
            .wrap_err("Expected ETag header to be in the response")?
            .to_str()?
            .to_owned()
            .replace('"', ""),
        version_id: Option::transpose(headers.get("x-amz-version-id").map(HeaderValue::to_str))?
            .map(ToOwned::to_owned),
        expiration: Option::transpose(headers.get("x-amz-expiration").map(HeaderValue::to_str))?
            .map(ToOwned::to_owned),
    };

    let backup_size = format_size(info.size, humansize::BINARY);
    tracing::debug!("{info:#?}");
    tracing::info!(
        "Completed backup to s3! (entity: \"{}\", version: \"{}\", size: {backup_size})",
        info.entity_tag,
        info.version_id.clone().unwrap_or_default()
    );
    Ok(info)
}

pub struct Config {
    pub client: reqwest::Client,
    pub backup: &'static options::Backup,
    pub aws_secret_access_key: &'static SecretString,
    pub database: Database,
}

pub fn spawn_backup_task(config: Config) {
    let span = tracing::error_span!("backup");
    tokio::spawn(
        async move {
            let mut initial = true;
            loop {
                'retry: loop {
                    match perform_backup(&config).await.wrap_err_with(|| {
                        if initial {
                            "Error performing initial backup"
                        } else {
                            "Error performing recurring backup"
                        }
                    }) {
                        Ok(_) => break 'retry,
                        Err(error) => tracing::error!("{error}"),
                    }
                    tracing::warn!("Retrying in 30 seconds...");
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    tracing::warn!("Retrying..");
                }

                let next_time = config.backup.schedule.next_time_from_now();
                let now = OffsetDateTime::now_utc();
                let duration: Duration = (next_time - now)
                    .try_into()
                    .expect("Unable to convert duration");
                let human_duration = humantime::format_duration(duration.clone());
                tracing::info!("Next backup in {human_duration}");
                tokio::time::sleep(duration).await;
                initial = false;
            }
        }
        .instrument(span),
    );
}
