use crate::errors::{DownloadError, UploadError};
use google_cloud_gax::paginator::ItemPaginator;
use google_cloud_storage::client::{Storage, StorageControl};
use std::path::PathBuf;
use tokio::runtime::Runtime;
use tracing::{debug, info};

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
    // Automatically handles authentication via GOOGLE_APPLICATION_CREDENTIALS
    let storage = Storage::builder()
        .build()
        .await
        .map_err(|e| DownloadError::Gcs(e.to_string()))?;

    let storage_control = StorageControl::builder()
        .build()
        .await
        .map_err(|e| DownloadError::Gcs(e.to_string()))?;

    let mut all_objects = vec![];

    let parent_name = format!("projects/_/buckets/{}", bucket);
    debug!(
        "Listing objects in GCS bucket: {}, parent: {}",
        bucket, parent_name
    );
    let mut list_request = storage_control.list_objects().set_parent(parent_name);
    if let Some(p) = prefix {
        list_request = list_request.set_prefix(p.to_string());
    }

    let mut stream = list_request.by_item();
    while let Some(obj) = stream
        .next()
        .await
        .transpose()
        .map_err(|e| DownloadError::Gcs(e.to_string()))?
    {
        if check_model_extension(&obj.name) {
            all_objects.push(obj.name);
        }
    }
    debug!("Found {} model files", all_objects.len());

    let cache_dir = get_cache_dir(bucket, prefix);
    tokio::fs::create_dir_all(&cache_dir)
        .await
        .map_err(DownloadError::Io)?;

    let mut downloaded_files = Vec::new();

    for object_name in all_objects {
        let filename = object_name.rsplit('/').next().unwrap_or(&object_name);
        let local_path = cache_dir.join(filename);

        if local_path.exists() {
            downloaded_files.push(local_path);
            continue;
        }

        let bucket_resource_name = format!("projects/_/buckets/{}", bucket);
        let mut read_response = storage
            .read_object(&bucket_resource_name, &object_name)
            .send()
            .await
            .map_err(|e| DownloadError::Gcs(e.to_string()))?;

        let mut data = Vec::new();
        while let Some(chunk_result) = read_response.next().await {
            let chunk = chunk_result.map_err(|arg0: google_cloud_storage::Error| {
                DownloadError::Gcs(arg0.to_string())
            })?;
            data.extend_from_slice(&chunk);
        }

        tokio::fs::write(&local_path, &data)
            .await
            .map_err(DownloadError::Io)?;
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
    cancellation_token: tokio_util::sync::CancellationToken,
) -> Result<(), UploadError> {
    let storage = Storage::builder()
        .build()
        .await
        .map_err(|e| UploadError::Gcs(e.to_string()))?;

    for path in local {
        if cancellation_token.is_cancelled() {
            return Ok(());
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| UploadError::InvalidFilename(path.clone()))?;
        let object_name = match &gcs_info.gcs_prefix {
            Some(p) => format!("{}/{}", p.trim_end_matches('/'), file_name),
            None => file_name.to_string(),
        };
        let bucket_resource_name = format!("projects/_/buckets/{}", gcs_info.gcs_bucket);

        let data = bytes::Bytes::from(tokio::fs::read(&path).await.map_err(UploadError::Io)?);

        let uploaded_file = storage
            .write_object(&bucket_resource_name, &object_name, data)
            .send_unbuffered()
            .await
            .map_err(|e| UploadError::Gcs(e.to_string()))?;
        info!(object = %object_name, size = uploaded_file.size, "Uploaded");
    }

    Ok(())
}
