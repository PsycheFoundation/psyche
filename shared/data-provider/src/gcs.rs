use crate::errors::{DownloadError, UploadError};
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::upload::Media;
use google_cloud_storage::http::objects::upload::UploadObjectRequest;
use google_cloud_storage::http::objects::upload::UploadType;
use google_cloud_storage::http::objects::{
    download::Range, get::GetObjectRequest, list::ListObjectsRequest,
};
use psyche_coordinator::model::{self, GcsRepo};
use psyche_core::FixedString;
use std::path::PathBuf;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tracing::info;

#[derive(Debug, Clone)]
pub struct GcsUploadInfo {
    pub gcs_bucket: String,
    pub gcs_prefix: Option<String>,
}

const MODEL_EXTENSIONS: [&str; 3] = [".safetensors", ".json", ".py"];

fn check_model_extension(filename: &str) -> bool {
    MODEL_EXTENSIONS.iter().any(|ext| filename.ends_with(ext))
}

fn get_cache_dir(bucket: &str, prefix: Option<&str>) -> PathBuf {
    let base = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".cache"))
        .unwrap_or_else(|_| PathBuf::from(".cache"))
        .join("psyche")
        .join("gcs")
        .join(bucket);

    match prefix {
        Some(p) => base.join(p.trim_end_matches('/')),
        None => base,
    }
}

pub async fn download_model_from_gcs_async(
    bucket: &str,
    prefix: Option<&str>,
) -> Result<Vec<PathBuf>, DownloadError> {
    // Use authenticated client if GOOGLE_APPLICATION_CREDENTIALS is set, otherwise anonymous
    let config = if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
        info!("Using authenticated GCS client");
        ClientConfig::default().with_auth().await?
    } else {
        info!("Using anonymous GCS client");
        ClientConfig::default().anonymous()
    };
    let client = Client::new(config);

    // List all objects in the bucket with optional prefix
    let mut all_objects = vec![];
    let mut page_token: Option<String> = None;

    loop {
        let results = client
            .list_objects(&ListObjectsRequest {
                bucket: bucket.to_owned(),
                prefix: prefix.map(|s| s.to_owned()),
                page_token: page_token.clone(),
                ..Default::default()
            })
            .await?;

        for obj in results.items.iter().flatten() {
            if check_model_extension(&obj.name) {
                all_objects.push(obj.name.clone());
            }
        }

        match results.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
        }
    }

    info!(
        "Found {} model files in gs://{}/{}",
        all_objects.len(),
        bucket,
        prefix.unwrap_or("")
    );

    let cache_dir = get_cache_dir(bucket, prefix);
    std::fs::create_dir_all(&cache_dir)?;

    let mut downloaded_files = Vec::new();

    for object_name in all_objects {
        // Get just the filename (strip prefix if present)
        let filename = object_name.rsplit('/').next().unwrap_or(&object_name);

        let local_path = cache_dir.join(filename);

        // Skip if already cached
        if local_path.exists() {
            info!("Using cached: {}", filename);
            downloaded_files.push(local_path);
            continue;
        }

        info!("Downloading: {}", object_name);

        // Download the object
        let data = client
            .download_object(
                &GetObjectRequest {
                    bucket: bucket.to_owned(),
                    object: object_name.clone(),
                    ..Default::default()
                },
                &Range::default(),
            )
            .await?;

        // Write to cache
        std::fs::write(&local_path, &data)?;

        info!("Downloaded: {} ({} bytes)", filename, data.len());

        downloaded_files.push(local_path);
    }

    Ok(downloaded_files)
}

pub fn download_model_from_gcs_sync(
    bucket: &str,
    prefix: Option<&str>,
) -> Result<Vec<PathBuf>, DownloadError> {
    let rt = Runtime::new().map_err(DownloadError::Io)?;
    rt.block_on(download_model_from_gcs_async(bucket, prefix))
}

pub async fn upload_to_gcs(
    gcs_info: GcsUploadInfo,
    local: Vec<PathBuf>,
    step: u64,
    tx_checkpoint: mpsc::UnboundedSender<model::Checkpoint>,
) -> Result<(), UploadError> {
    let GcsUploadInfo {
        gcs_bucket,
        gcs_prefix,
    } = gcs_info;

    info!(bucket = gcs_bucket, "Uploading checkpoint to GCS");

    let config = if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
        info!("Using authenticated GCS client");
        ClientConfig::default().with_auth().await?
    } else {
        info!("Using anonymous GCS client");
        ClientConfig::default().anonymous()
    };
    let client = Client::new(config);

    for path in local {
        let file_name = path
            .file_name()
            .ok_or_else(|| UploadError::NotAFile(path.clone()))?
            .to_str()
            .ok_or_else(|| UploadError::InvalidFilename(path.clone()))?;

        let object_name = match &gcs_prefix {
            Some(p) => format!("{}/{}", p, file_name),
            None => file_name.to_string(),
        };

        let data = tokio::fs::read(&path).await?;

        let upload_type = UploadType::Simple(Media::new(object_name.clone()));
        let uploaded = client
            .upload_object(
                &UploadObjectRequest {
                    bucket: gcs_bucket.clone(),
                    ..Default::default()
                },
                data,
                &upload_type,
            )
            .await?;

        info!(
            bucket = gcs_bucket,
            object = object_name,
            size = uploaded.size,
            "Successfully uploaded file to GCS"
        );
    }

    info!(
        "Upload to GCS complete at gs://{}/{}",
        gcs_bucket,
        gcs_prefix.as_deref().unwrap_or("")
    );

    tx_checkpoint
        .send(model::Checkpoint::Gcs(GcsRepo {
            bucket: FixedString::from_str_truncated(&format!(
                "gs://{}/{}",
                gcs_bucket,
                gcs_prefix.as_deref().unwrap_or("")
            )),
            prefix: Some(FixedString::from_str_truncated(&format!("step-{step}"))),
        }))
        .map_err(|_| UploadError::SendCheckpoint)?;

    Ok(())
}
