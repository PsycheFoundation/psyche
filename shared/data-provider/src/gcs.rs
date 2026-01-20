use crate::errors::{DownloadError, UploadError};
use chrono::{DateTime, Utc};
use google_cloud_gax::paginator::ItemPaginator;
use google_cloud_storage::client::{Storage, StorageControl};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::runtime::Runtime;
use tracing::{debug, info};

/// Checkpoint manifest.json uploaded to GCS alongside safetensors files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcsCheckpointManifest {
    pub metadata: ManifestMetadata,
    pub files: Vec<ManifestFileEntry>,
}

/// Checkpoint metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestMetadata {
    pub timestamp: DateTime<Utc>,
    pub epoch: u32,
    pub step: u32,
    pub run_id: String,
}

/// Single file entry in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFileEntry {
    pub filename: String,
    pub generation: i64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct GcsUploadInfo {
    pub gcs_bucket: String,
    pub gcs_prefix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GcsManifestMetadata {
    pub epoch: u32,
    pub run_id: String,
}

const MODEL_EXTENSIONS: [&str; 3] = [".safetensors", ".json", ".py"];

fn get_cache_base(bucket: &str) -> PathBuf {
    // Use HF_HOME if set, otherwise fall back to ~/.cache
    std::env::var("HF_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".cache"))
                .unwrap_or_else(|_| PathBuf::from(".cache"))
        })
        .join("psyche")
        .join("gcs")
        .join(bucket)
}

fn get_cache_dir(
    bucket: &str,
    prefix: Option<&str>,
    step: u32,
    manifest_generation: i64,
) -> PathBuf {
    let base = get_cache_base(bucket);
    let versioned_folder = format!("step-{}-{}", step, manifest_generation);

    match prefix {
        Some(p) => base.join(p.trim_end_matches('/')).join(versioned_folder),
        None => base.join(versioned_folder),
    }
}

fn get_cache_dir_no_manifest(bucket: &str, prefix: Option<&str>) -> PathBuf {
    let base = get_cache_base(bucket);

    match prefix {
        Some(p) => base.join(p.trim_end_matches('/')).join("no_manifest"),
        None => base.join("no_manifest"),
    }
}

fn collect_cached_files(
    cache_dir: &Path,
    manifest: &GcsCheckpointManifest,
) -> Option<Vec<PathBuf>> {
    let mut files = Vec::new();
    for file_entry in &manifest.files {
        let path = cache_dir.join(&file_entry.filename);
        if !path.exists() {
            return None;
        }
        files.push(path);
    }
    Some(files)
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

    let manifest_object_path = match prefix {
        Some(p) => format!("{}/manifest.json", p),
        None => "manifest.json".to_string(),
    };

    // Try to get manifest - first check if it exists
    let bucket_resource_name = format!("projects/_/buckets/{}", bucket);
    let manifest_result = storage
        .read_object(&bucket_resource_name, &manifest_object_path)
        .send()
        .await;

    match manifest_result {
        Ok(mut read_response) => {
            // Read manifest content
            let mut manifest_data = Vec::new();
            while let Some(chunk_result) = read_response.next().await {
                let chunk = chunk_result.map_err(|e| DownloadError::Gcs(e.to_string()))?;
                manifest_data.extend_from_slice(&chunk);
            }

            let manifest: GcsCheckpointManifest = serde_json::from_slice(&manifest_data)?;

            // Use step as generation proxy (1.5.x doesn't expose generation in same way)
            let manifest_generation = manifest.metadata.step as i64;

            info!(
                "Found manifest: step {}, epoch {}, generation {}",
                manifest.metadata.step, manifest.metadata.epoch, manifest_generation
            );

            // Build versioned cache path
            let cache_dir =
                get_cache_dir(bucket, prefix, manifest.metadata.step, manifest_generation);

            // Check if all manifest files exist in cache
            let mut files = if let Some(cached) = collect_cached_files(&cache_dir, &manifest) {
                info!("Using cached checkpoint at {:?}", cache_dir);
                cached
            } else {
                info!(
                    "Model not found in cache, downloading checkpoint to {:?}",
                    cache_dir
                );
                std::fs::create_dir_all(&cache_dir)?;
                download_files_from_manifest(&storage, bucket, prefix, &cache_dir, &manifest)
                    .await?
            };
            // Download config files (json, py) - skips if already cached
            let config_files = download_files_no_manifest(
                &storage_control,
                &storage,
                bucket,
                prefix,
                &cache_dir,
                &[".json", ".py"],
            )
            .await?;
            files.extend(config_files);
            Ok(files)
        }
        Err(_) => {
            // Fallback for old checkpoints without manifest
            info!("No manifest found, downloading model without manifest");
            let cache_dir = get_cache_dir_no_manifest(bucket, prefix);
            std::fs::create_dir_all(&cache_dir)?;
            download_files_no_manifest(
                &storage_control,
                &storage,
                bucket,
                prefix,
                &cache_dir,
                &MODEL_EXTENSIONS,
            )
            .await
        }
    }
}

async fn download_files_from_manifest(
    storage: &Storage,
    bucket: &str,
    prefix: Option<&str>,
    cache_dir: &Path,
    manifest: &GcsCheckpointManifest,
) -> Result<Vec<PathBuf>, DownloadError> {
    let mut downloaded_files = Vec::new();
    let bucket_resource_name = format!("projects/_/buckets/{}", bucket);

    for file_entry in &manifest.files {
        let object_name = match prefix {
            Some(p) => format!("{}/{}", p, file_entry.filename),
            None => file_entry.filename.clone(),
        };
        let local_path = cache_dir.join(&file_entry.filename);

        if local_path.exists() {
            info!("Using cached: {}", file_entry.filename);
            downloaded_files.push(local_path);
            continue;
        }

        info!(
            "Downloading: gs://{}/{} (generation {})",
            bucket, object_name, file_entry.generation
        );

        let mut read_response = storage
            .read_object(&bucket_resource_name, &object_name)
            .send()
            .await
            .map_err(|e| DownloadError::Gcs(e.to_string()))?;

        let mut data = Vec::new();
        while let Some(chunk_result) = read_response.next().await {
            let chunk = chunk_result.map_err(|e| DownloadError::Gcs(e.to_string()))?;
            data.extend_from_slice(&chunk);
        }

        std::fs::write(&local_path, &data)?;
        info!("Downloaded: {} ({} bytes)", file_entry.filename, data.len());
        downloaded_files.push(local_path);
    }

    Ok(downloaded_files)
}

/// Download model files by listing the bucket. Skips files that already exist in cache.
/// Used for initial model download (no manifest) and to fetch config files (json, py) after manifest download.
async fn download_files_no_manifest(
    storage_control: &StorageControl,
    storage: &Storage,
    bucket: &str,
    prefix: Option<&str>,
    cache_dir: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>, DownloadError> {
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
        if extensions.iter().any(|ext| obj.name.ends_with(ext)) {
            all_objects.push(obj.name);
        }
    }

    info!(
        "Found {} files ({}) in gs://{}/{}",
        all_objects.len(),
        extensions.join(", "),
        bucket,
        prefix.unwrap_or("")
    );

    let mut downloaded_files = Vec::new();

    for object_name in all_objects {
        let filename = object_name.rsplit('/').next().unwrap_or(&object_name);
        let local_path = cache_dir.join(filename);

        if local_path.exists() {
            info!("Using cached: {}", filename);
            downloaded_files.push(local_path);
            continue;
        }

        info!("Downloading: gs://{}/{}", bucket, object_name);

        let bucket_resource_name = format!("projects/_/buckets/{}", bucket);
        let mut read_response = storage
            .read_object(&bucket_resource_name, &object_name)
            .send()
            .await
            .map_err(|e| DownloadError::Gcs(e.to_string()))?;

        let mut data = Vec::new();
        while let Some(chunk_result) = read_response.next().await {
            let chunk = chunk_result.map_err(|e| DownloadError::Gcs(e.to_string()))?;
            data.extend_from_slice(&chunk);
        }

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
    manifest_metadata: GcsManifestMetadata,
    local: Vec<PathBuf>,
    step: u64,
    cancellation_token: tokio_util::sync::CancellationToken,
) -> Result<(), UploadError> {
    let storage = Storage::builder()
        .build()
        .await
        .map_err(|e| UploadError::Gcs(e.to_string()))?;

    let mut manifest = GcsCheckpointManifest {
        metadata: ManifestMetadata {
            timestamp: Utc::now(),
            epoch: manifest_metadata.epoch,
            step: step as u32,
            run_id: manifest_metadata.run_id,
        },
        files: Vec::new(),
    };

    for path in local
        .iter()
        .filter(|p| p.extension() == Some("safetensors".as_ref()))
    {
        if cancellation_token.is_cancelled() {
            info!("Upload cancelled before uploading {}", path.display());
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

        let data_vec = tokio::fs::read(&path).await?;
        let size = data_vec.len() as u64;
        let data = bytes::Bytes::from(data_vec);

        let upload_future = storage
            .write_object(&bucket_resource_name, &object_name, data)
            .send_unbuffered();

        let uploaded_file = tokio::select! {
            biased;

            _ = cancellation_token.cancelled() => {
                info!("Upload cancelled during upload of {}", path.display());
                return Ok(());
            }
            result = upload_future => {
                result.map_err(|e| UploadError::Gcs(e.to_string()))?
            }
        };

        info!(
            bucket = gcs_info.gcs_bucket,
            object = object_name,
            size = uploaded_file.size,
            "Successfully uploaded file to GCS"
        );

        manifest.files.push(ManifestFileEntry {
            filename: file_name.to_string(),
            generation: uploaded_file.generation,
            size_bytes: size,
        });
    }

    // Upload the manifest file
    let manifest_path = match &gcs_info.gcs_prefix {
        Some(p) => format!("{}/manifest.json", p),
        None => "manifest.json".to_string(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    let manifest_bytes = bytes::Bytes::from(manifest_json.into_bytes());

    let bucket_resource_name = format!("projects/_/buckets/{}", gcs_info.gcs_bucket);
    storage
        .write_object(&bucket_resource_name, &manifest_path, manifest_bytes)
        .send_unbuffered()
        .await
        .map_err(|e| UploadError::Gcs(e.to_string()))?;

    info!(
        bucket = gcs_info.gcs_bucket,
        object = manifest_path,
        "Uploaded manifest to GCS"
    );

    info!(
        "Upload to GCS complete at gs://{}/{}",
        gcs_info.gcs_bucket,
        gcs_info.gcs_prefix.as_deref().unwrap_or("")
    );

    Ok(())
}
