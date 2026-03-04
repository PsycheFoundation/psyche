use crate::errors::{DownloadError, UploadError};
use crate::gcs::{
    GcsCheckpointManifest, GcsManifestMetadata, MODEL_EXTENSIONS, ManifestFileEntry,
    ManifestMetadata, collect_cached_files, get_cache_dir, get_cache_dir_no_manifest,
};
use crate::run_down::{DownloadUrlEntry, RunDownClient};
use chrono::Utc;
use futures::TryStreamExt;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tracing::info;

pub async fn upload_to_gcs_signed(
    run_down: &RunDownClient,
    manifest_metadata: GcsManifestMetadata,
    local: Vec<PathBuf>,
    step: u64,
    cancellation_token: tokio_util::sync::CancellationToken,
) -> Result<(), UploadError> {
    let http = reqwest::Client::new();

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

        let file = tokio::fs::File::open(&path).await?;
        let size = file.metadata().await?.len();

        let upload_url = run_down
            .get_upload_url(file_name)
            .await
            .map_err(|e| UploadError::RunDown(e.to_string()))?;

        info!(file = file_name, size, "Uploading file via signed URL");

        let upload_future = http
            .put(&upload_url.url)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", size)
            .body(reqwest::Body::from(file))
            .send();

        let response = tokio::select! {
            biased;

            _ = cancellation_token.cancelled() => {
                info!("Upload cancelled during upload of {}", path.display());
                return Ok(());
            }
            result = upload_future => {
                result.map_err(|e| UploadError::RunDown(e.to_string()))?
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(UploadError::RunDown(format!(
                "Signed URL upload failed for {}: {} {}",
                file_name, status, error_text
            )));
        }

        let generation = response
            .headers()
            .get("x-goog-generation")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0);

        info!(
            file = file_name,
            size, generation, "Successfully uploaded file via signed URL"
        );

        manifest.files.push(ManifestFileEntry {
            filename: file_name.to_string(),
            generation,
            size_bytes: size,
        });
    }

    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    let manifest_bytes = manifest_json.into_bytes();

    let manifest_upload_url = run_down
        .get_upload_url("manifest.json")
        .await
        .map_err(|e| UploadError::RunDown(e.to_string()))?;

    let response = http
        .put(&manifest_upload_url.url)
        .header("Content-Type", "application/json")
        .body(manifest_bytes)
        .send()
        .await
        .map_err(|e| UploadError::RunDown(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(UploadError::RunDown(format!(
            "Signed URL upload failed for manifest.json: {} {}",
            status, error_text
        )));
    }

    info!(
        run_id = run_down.run_id(),
        "Upload via signed URLs complete"
    );

    Ok(())
}

pub async fn download_model_from_gcs_signed_async(
    run_down: &RunDownClient,
) -> Result<Vec<PathBuf>, DownloadError> {
    let http = reqwest::Client::new();
    let run_id = run_down.run_id();

    let download_response = run_down
        .get_download_urls()
        .await
        .map_err(|e| DownloadError::RunDown(e.to_string()))?;

    info!(
        "Got {} download URLs from run-down for run {}",
        download_response.urls.len(),
        run_id
    );

    let manifest_entry = download_response
        .urls
        .iter()
        .find(|e| e.path.ends_with("manifest.json"));

    let cache_key = &format!("signed-urls/{}", run_id);

    match manifest_entry {
        Some(manifest_entry) => {
            let response = http
                .get(&manifest_entry.url)
                .send()
                .await
                .map_err(|e| DownloadError::RunDown(e.to_string()))?;

            if !response.status().is_success() {
                return Err(DownloadError::RunDown(format!(
                    "Failed to download manifest.json: {}",
                    response.status()
                )));
            }

            let manifest_data = response
                .bytes()
                .await
                .map_err(|e| DownloadError::RunDown(e.to_string()))?;

            let manifest: GcsCheckpointManifest = serde_json::from_slice(&manifest_data)?;
            let manifest_generation = manifest.metadata.step as i64;

            info!(
                "Found manifest: step {}, epoch {}, generation {}",
                manifest.metadata.step, manifest.metadata.epoch, manifest_generation
            );

            let cache_dir =
                get_cache_dir(cache_key, None, manifest.metadata.step, manifest_generation);

            let mut files = if let Some(cached) = collect_cached_files(&cache_dir, &manifest) {
                info!("Using cached checkpoint at {:?}", cache_dir);
                cached
            } else {
                info!("Downloading checkpoint via signed URLs to {:?}", cache_dir);
                std::fs::create_dir_all(&cache_dir)?;
                download_files_from_signed_urls(
                    &http,
                    &download_response.urls,
                    &cache_dir,
                    &manifest,
                )
                .await?
            };

            let config_files = download_non_manifest_files_from_signed_urls(
                &http,
                &download_response.urls,
                &cache_dir,
                &[".json", ".py"],
                &manifest,
            )
            .await?;
            files.extend(config_files);

            Ok(files)
        }
        None => {
            info!("No manifest found in signed URLs, downloading all model files");
            let cache_dir = get_cache_dir_no_manifest(cache_key, None);
            std::fs::create_dir_all(&cache_dir)?;
            download_all_model_files_from_signed_urls(
                &http,
                &download_response.urls,
                &cache_dir,
                &MODEL_EXTENSIONS,
            )
            .await
        }
    }
}

async fn download_files_from_signed_urls(
    http: &reqwest::Client,
    urls: &[DownloadUrlEntry],
    cache_dir: &Path,
    manifest: &GcsCheckpointManifest,
) -> Result<Vec<PathBuf>, DownloadError> {
    let mut downloaded_files = Vec::new();

    for file_entry in &manifest.files {
        let local_path = cache_dir.join(&file_entry.filename);

        if local_path.exists() {
            info!("Using cached: {}", file_entry.filename);
            downloaded_files.push(local_path);
            continue;
        }

        let url_entry = urls
            .iter()
            .find(|e| e.path.ends_with(&file_entry.filename))
            .ok_or_else(|| {
                DownloadError::RunDown(format!(
                    "No signed URL found for file: {}",
                    file_entry.filename
                ))
            })?;

        info!("Downloading via signed URL: {}", file_entry.filename);

        let response = http
            .get(&url_entry.url)
            .send()
            .await
            .map_err(|e| DownloadError::RunDown(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DownloadError::RunDown(format!(
                "Failed to download {}: {}",
                file_entry.filename,
                response.status()
            )));
        }

        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&local_path)
            .await
            .map_err(|e| DownloadError::Io(e))?;
        while let Some(chunk) = stream
            .try_next()
            .await
            .map_err(|e| DownloadError::RunDown(e.to_string()))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|e| DownloadError::Io(e))?;
        }
        info!("Downloaded: {}", file_entry.filename);
        downloaded_files.push(local_path);
    }

    Ok(downloaded_files)
}

async fn download_non_manifest_files_from_signed_urls(
    http: &reqwest::Client,
    urls: &[DownloadUrlEntry],
    cache_dir: &Path,
    extensions: &[&str],
    manifest: &GcsCheckpointManifest,
) -> Result<Vec<PathBuf>, DownloadError> {
    let manifest_filenames: std::collections::HashSet<&str> =
        manifest.files.iter().map(|f| f.filename.as_str()).collect();

    let mut downloaded_files = Vec::new();

    for url_entry in urls {
        let filename = url_entry.path.rsplit('/').next().unwrap_or(&url_entry.path);

        if manifest_filenames.contains(filename) {
            continue;
        }

        if !extensions.iter().any(|ext| filename.ends_with(ext)) {
            continue;
        }

        let local_path = cache_dir.join(filename);
        if local_path.exists() {
            info!("Using cached: {}", filename);
            downloaded_files.push(local_path);
            continue;
        }

        info!("Downloading config via signed URL: {}", filename);

        let response = http
            .get(&url_entry.url)
            .send()
            .await
            .map_err(|e| DownloadError::RunDown(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DownloadError::RunDown(format!(
                "Failed to download {}: {}",
                filename,
                response.status()
            )));
        }

        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&local_path)
            .await
            .map_err(|e| DownloadError::Io(e))?;
        while let Some(chunk) = stream
            .try_next()
            .await
            .map_err(|e| DownloadError::RunDown(e.to_string()))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|e| DownloadError::Io(e))?;
        }
        info!("Downloaded: {}", filename);
        downloaded_files.push(local_path);
    }

    Ok(downloaded_files)
}

async fn download_all_model_files_from_signed_urls(
    http: &reqwest::Client,
    urls: &[DownloadUrlEntry],
    cache_dir: &Path,
    extensions: &[&str],
) -> Result<Vec<PathBuf>, DownloadError> {
    let mut downloaded_files = Vec::new();

    for url_entry in urls {
        let filename = url_entry.path.rsplit('/').next().unwrap_or(&url_entry.path);

        if !extensions.iter().any(|ext| filename.ends_with(ext)) {
            continue;
        }

        let local_path = cache_dir.join(filename);
        if local_path.exists() {
            info!("Using cached: {}", filename);
            downloaded_files.push(local_path);
            continue;
        }

        info!("Downloading via signed URL: {}", filename);

        let response = http
            .get(&url_entry.url)
            .send()
            .await
            .map_err(|e| DownloadError::RunDown(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DownloadError::RunDown(format!(
                "Failed to download {}: {}",
                filename,
                response.status()
            )));
        }

        let mut stream = response.bytes_stream();
        let mut file = tokio::fs::File::create(&local_path)
            .await
            .map_err(|e| DownloadError::Io(e))?;
        while let Some(chunk) = stream
            .try_next()
            .await
            .map_err(|e| DownloadError::RunDown(e.to_string()))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|e| DownloadError::Io(e))?;
        }
        info!("Downloaded: {}", filename);
        downloaded_files.push(local_path);
    }

    Ok(downloaded_files)
}
