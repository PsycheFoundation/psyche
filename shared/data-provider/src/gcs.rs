use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::{
    download::Range, get::GetObjectRequest, list::ListObjectsRequest,
};
use std::path::PathBuf;
use thiserror::Error;
use tokio::runtime::Runtime;
use tracing::info;

#[derive(Debug, Error)]
pub enum GcsError {
    #[error("GCS authentication failed: {0}")]
    Auth(#[from] google_cloud_storage::client::google_cloud_auth::error::Error),

    #[error("GCS operation failed: {0}")]
    Storage(#[from] google_cloud_storage::http::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
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
) -> Result<Vec<PathBuf>, GcsError> {
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
) -> Result<Vec<PathBuf>, GcsError> {
    let rt = Runtime::new().map_err(GcsError::Io)?;
    rt.block_on(download_model_from_gcs_async(bucket, prefix))
}
