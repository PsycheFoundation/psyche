use crate::errors::{DownloadError, UploadError};
use chrono::{DateTime, Utc};
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::upload::Media;
use google_cloud_storage::http::objects::upload::UploadObjectRequest;
use google_cloud_storage::http::objects::upload::UploadType;
use google_cloud_storage::http::objects::{
    download::Range, get::GetObjectRequest, list::ListObjectsRequest,
};
use psyche_coordinator::model::{self, GcsRepo};
use psyche_core::FixedString;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tracing::info;

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
    // Use authenticated client if GOOGLE_APPLICATION_CREDENTIALS is set, otherwise anonymous
    let config = if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
        info!("Using authenticated GCS client");
        ClientConfig::default().with_auth().await?
    } else {
        info!("Using anonymous GCS client");
        ClientConfig::default().anonymous()
    };
    let client = Client::new(config);

    let manifest_object_path = match prefix {
        Some(p) => format!("{}/manifest.json", p),
        None => "manifest.json".to_string(),
    };

    // Get manifest metadata to obtain generation number
    let manifest_metadata = client
        .get_object(&GetObjectRequest {
            bucket: bucket.to_owned(),
            object: manifest_object_path.clone(),
            ..Default::default()
        })
        .await;

    match manifest_metadata {
        Ok(object_meta) => {
            let manifest_generation = object_meta.generation;

            // Download manifest content
            let manifest_data = client
                .download_object(
                    &GetObjectRequest {
                        bucket: bucket.to_owned(),
                        object: manifest_object_path,
                        ..Default::default()
                    },
                    &Range::default(),
                )
                .await?;

            let manifest: GcsCheckpointManifest = serde_json::from_slice(&manifest_data)?;

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
                download_files_from_manifest(&client, bucket, prefix, &cache_dir, &manifest).await?
            };
            // Download config files (json, py) - skips if already cached
            let config_files =
                download_files_no_manifest(&client, bucket, prefix, &cache_dir, &[".json", ".py"])
                    .await?;
            files.extend(config_files);
            Ok(files)
        }
        Err(_) => {
            // Fallback for old checkpoints without manifest
            info!("No manifest found, downloading model without manifest");
            let cache_dir = get_cache_dir_no_manifest(bucket, prefix);
            std::fs::create_dir_all(&cache_dir)?;
            download_files_no_manifest(&client, bucket, prefix, &cache_dir, &MODEL_EXTENSIONS).await
        }
    }
}

async fn download_files_from_manifest(
    client: &Client,
    bucket: &str,
    prefix: Option<&str>,
    cache_dir: &Path,
    manifest: &GcsCheckpointManifest,
) -> Result<Vec<PathBuf>, DownloadError> {
    let mut downloaded_files = Vec::new();

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

        let data = client
            .download_object(
                &GetObjectRequest {
                    bucket: bucket.to_owned(),
                    object: object_name,
                    generation: Some(file_entry.generation),
                    ..Default::default()
                },
                &Range::default(),
            )
            .await?;

        std::fs::write(&local_path, &data)?;
        info!("Downloaded: {} ({} bytes)", file_entry.filename, data.len());
        downloaded_files.push(local_path);
    }

    Ok(downloaded_files)
}

/// Download model files by listing the bucket. Skips files that already exist in cache.
/// Used for initial model download (no manifest) and to fetch config files (json, py) after manifest download.
async fn download_files_no_manifest(
    client: &Client,
    bucket: &str,
    prefix: Option<&str>,
    cache_dir: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>, DownloadError> {
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
            if extensions.iter().any(|ext| obj.name.ends_with(ext)) {
                all_objects.push(obj.name.clone());
            }
        }

        match results.next_page_token {
            Some(token) => page_token = Some(token),
            None => break,
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
    manifest_metadata: GcsManifestMetadata,
    local: Vec<PathBuf>,
    step: u64,
    tx_checkpoint: mpsc::UnboundedSender<model::Checkpoint>,
) -> Result<(), UploadError> {
    let GcsUploadInfo {
        gcs_bucket,
        gcs_prefix,
    } = gcs_info;

    let GcsManifestMetadata { epoch, run_id } = manifest_metadata;

    info!(bucket = gcs_bucket, "Uploading checkpoint to GCS");

    let config = if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
        info!("Using authenticated GCS client");
        ClientConfig::default().with_auth().await?
    } else {
        info!("Using anonymous GCS client");
        ClientConfig::default().anonymous()
    };
    let client = Client::new(config);

    let mut manifest = GcsCheckpointManifest {
        metadata: ManifestMetadata {
            timestamp: Utc::now(),
            epoch,
            step: step as u32,
            run_id,
        },
        files: Vec::new(),
    };

    for path in local {
        let file_name = path
            .file_name()
            .ok_or_else(|| UploadError::NotAFile(path.clone()))?
            .to_str()
            .ok_or_else(|| UploadError::InvalidFilename(path.clone()))?;

        // Only upload safetensors files
        if !file_name.ends_with(".safetensors") {
            continue;
        }

        let object_name = match &gcs_prefix {
            Some(p) => format!("{}/{}", p, file_name),
            None => file_name.to_string(),
        };

        let size = std::fs::metadata(&path)?.len();
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
            generation = uploaded.generation,
            "Uploaded file to GCS"
        );

        manifest.files.push(ManifestFileEntry {
            filename: file_name.to_string(),
            generation: uploaded.generation,
            size_bytes: size,
        });
    }

    // Upload the manifest file
    let manifest_path = match &gcs_prefix {
        Some(p) => format!("{}/manifest.json", p),
        None => "manifest.json".to_string(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)?;

    let upload_type = UploadType::Simple(Media::new(manifest_path.clone()));
    client
        .upload_object(
            &UploadObjectRequest {
                bucket: gcs_bucket.clone(),
                ..Default::default()
            },
            manifest_json.into_bytes(),
            &upload_type,
        )
        .await?;

    info!(
        bucket = gcs_bucket,
        object = manifest_path,
        "Uploaded manifest to GCS"
    );

    info!(
        "Upload to GCS complete at gs://{}/{}",
        gcs_bucket,
        gcs_prefix.as_deref().unwrap_or("")
    );

    tx_checkpoint
        .send(model::Checkpoint::Gcs(GcsRepo {
            bucket: FixedString::from_str_truncated(&gcs_bucket),
            prefix: gcs_prefix.map(|p| FixedString::from_str_truncated(&p)),
        }))
        .map_err(|_| UploadError::SendCheckpoint)?;

    Ok(())
}
