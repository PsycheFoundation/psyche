use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::{
    download::Range, get::GetObjectRequest, list::ListObjectsRequest,
};
use std::path::PathBuf;
use tokio::runtime::Runtime;
use tracing::info;

const MODEL_EXTENSIONS: [&str; 3] = [".safetensors", ".json", ".py"];

fn check_model_extension(filename: &str) -> bool {
    MODEL_EXTENSIONS.iter().any(|ext| filename.ends_with(ext))
}

fn get_cache_dir(bucket: &str, prefix: Option<&str>) -> PathBuf {
    let base = std::env::var("PSYCHE_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".cache"))
                .unwrap_or_else(|_| PathBuf::from(".cache"))
        })
        .join("psyche")
        .join("gcs")
        .join(bucket);

    match prefix {
        Some(p) => base.join(p.trim_end_matches('/')),
        None => base,
    }
}

async fn download_model_from_gcs_async(
    bucket: &str,
    prefix: Option<&str>,
    cache_dir: Option<PathBuf>,
    progress_bar: bool,
) -> Vec<PathBuf> {
    // Use authenticated client if GOOGLE_APPLICATION_CREDENTIALS is set, otherwise anonymous
    let config = if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
        if progress_bar {
            info!("Using authenticated GCS client");
        }
        ClientConfig::default().with_auth().await.unwrap()
    } else {
        if progress_bar {
            info!("Using anonymous GCS client");
        }
        ClientConfig::default().anonymous()
    };
    let client = Client::new(config);

    // List all objects in the bucket with optional prefix
    let mut all_objects = vec![];
    let mut next_page_token: Option<Option<String>> = Some(None);

    while let Some(maybe_next_page_token) = next_page_token {
        let results = client
            .list_objects(&ListObjectsRequest {
                bucket: bucket.to_owned(),
                prefix: prefix.map(|s| s.to_owned()),
                page_token: maybe_next_page_token,
                ..Default::default()
            })
            .await
            .unwrap();

        for obj in results.items.iter().flatten() {
            if check_model_extension(&obj.name) {
                all_objects.push(obj.name.clone());
            }
        }

        next_page_token = results.next_page_token.map(Some);
    }

    if progress_bar {
        info!(
            "Found {} model files in gs://{}/{}",
            all_objects.len(),
            bucket,
            prefix.unwrap_or("")
        );
    }

    // Determine cache directory
    let cache_dir = cache_dir.unwrap_or_else(|| get_cache_dir(bucket, prefix));
    std::fs::create_dir_all(&cache_dir).unwrap();

    let mut downloaded_files = Vec::new();

    for object_name in all_objects {
        // Get just the filename (strip prefix if present)
        let filename = object_name.rsplit('/').next().unwrap_or(&object_name);

        let local_path = cache_dir.join(filename);

        // Skip if already cached
        if local_path.exists() {
            if progress_bar {
                info!("Using cached: {}", filename);
            }
            downloaded_files.push(local_path);
            continue;
        }

        if progress_bar {
            info!("Downloading: {}", object_name);
        }

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
            .await
            .unwrap();

        // Write to cache
        std::fs::write(&local_path, &data).unwrap();

        if progress_bar {
            info!("Downloaded: {} ({} bytes)", filename, data.len());
        }

        downloaded_files.push(local_path);
    }

    downloaded_files
}

pub fn download_model_from_gcs_sync(
    bucket: &str,
    prefix: Option<&str>,
    cache_dir: Option<PathBuf>,
    progress_bar: bool,
) -> Vec<PathBuf> {
    let rt = Runtime::new().unwrap();
    rt.block_on(download_model_from_gcs_async(
        bucket,
        prefix,
        cache_dir,
        progress_bar,
    ))
}
