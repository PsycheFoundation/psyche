use crate::errors::UploadError;
use futures::future::try_join_all;
use hf_hub::{
    Cache, Repo, RepoType,
    api::{Siblings, tokio::ApiError},
};
use std::{path::PathBuf, time::Instant};
use tracing::{error, info};

const MODEL_EXTENSIONS: [&str; 3] = [".safetensors", ".json", ".py"];
const DATASET_EXTENSIONS: [&str; 1] = [".parquet"];

fn check_extensions(sibling: &Siblings, extensions: &[&'static str]) -> bool {
    match extensions.is_empty() {
        true => true,
        false => {
            for ext in extensions {
                if sibling.rfilename.ends_with(ext) {
                    return true;
                }
            }
            false
        }
    }
}

async fn download_repo_async(
    repo: Repo,
    cache: Option<PathBuf>,
    token: Option<String>,
    max_concurrent_downloads: Option<usize>,
    progress_bar: bool,
    extensions: &[&'static str],
) -> Result<Vec<PathBuf>, ApiError> {
    let builder = hf_hub::api::tokio::ApiBuilder::new();
    let cache = match cache {
        Some(cache) => Cache::new(cache),
        None => Cache::default(),
    };
    let api = builder
        .with_cache_dir(cache.path().clone())
        .with_token(token.or(cache.token()))
        .with_progress(progress_bar)
        .build()?
        .repo(repo);
    let siblings = api
        .info()
        .await?
        .siblings
        .into_iter()
        .filter(|x| check_extensions(x, extensions))
        .collect::<Vec<_>>();
    let mut ret: Vec<PathBuf> = Vec::new();
    for chunk in siblings.chunks(max_concurrent_downloads.unwrap_or(siblings.len())) {
        let futures = chunk.iter().map(|x| async {
            let start_time = Instant::now();
            tracing::debug!(filename = x.rfilename, "Starting file download from hub");
            let res = api.get(&x.rfilename).await;
            if res.is_ok() {
                let duration_secs = (Instant::now() - start_time).as_secs_f32();
                tracing::info!(
                    filename = x.rfilename,
                    duration_secs = duration_secs,
                    "Finished downloading file from hub"
                );
            }
            res
        });
        let chunk_results = try_join_all(futures).await?;
        ret.extend(chunk_results);
    }
    Ok(ret)
}

pub async fn download_model_repo_async(
    repo_id: &str,
    revision: Option<String>,
    cache: Option<PathBuf>,
    token: Option<String>,
    max_concurrent_downloads: Option<usize>,
    progress_bar: bool,
) -> Result<Vec<PathBuf>, ApiError> {
    download_repo_async(
        match revision {
            Some(revision) => Repo::with_revision(repo_id.to_string(), RepoType::Model, revision),
            None => Repo::model(repo_id.to_string()),
        },
        cache,
        token,
        max_concurrent_downloads,
        progress_bar,
        &MODEL_EXTENSIONS,
    )
    .await
}

pub async fn download_dataset_repo_async(
    repo_id: String,
    revision: Option<String>,
    cache: Option<PathBuf>,
    token: Option<String>,
    max_concurrent_downloads: Option<usize>,
    progress_bar: bool,
) -> Result<Vec<PathBuf>, ApiError> {
    download_repo_async(
        match revision {
            Some(revision) => Repo::with_revision(repo_id.to_owned(), RepoType::Dataset, revision),
            None => Repo::new(repo_id.to_owned(), RepoType::Dataset),
        },
        cache,
        token,
        max_concurrent_downloads,
        progress_bar,
        &DATASET_EXTENSIONS,
    )
    .await
}

fn download_repo_sync(
    repo: Repo,
    cache: Option<PathBuf>,
    token: Option<String>,
    progress_bar: bool,
    extensions: &[&'static str],
) -> Result<Vec<PathBuf>, hf_hub::api::sync::ApiError> {
    let builder = hf_hub::api::sync::ApiBuilder::new();
    let cache = match cache {
        Some(cache) => Cache::new(cache),
        None => Cache::default(),
    };
    let api = builder
        .with_cache_dir(cache.path().clone())
        .with_token(token.or(cache.token()))
        .with_progress(progress_bar)
        .build()?
        .repo(repo);
    let res: Result<Vec<PathBuf>, _> = api
        .info()?
        .siblings
        .into_iter()
        .filter(|x| check_extensions(x, extensions))
        .map(|x| api.get(&x.rfilename))
        .collect();

    res
}

pub fn download_model_repo_sync(
    repo_id: &str,
    revision: Option<String>,
    cache: Option<PathBuf>,
    token: Option<String>,
    progress_bar: bool,
) -> Result<Vec<PathBuf>, hf_hub::api::sync::ApiError> {
    download_repo_sync(
        match revision {
            Some(revision) => Repo::with_revision(repo_id.to_owned(), RepoType::Model, revision),
            None => Repo::model(repo_id.to_owned()),
        },
        cache,
        token,
        progress_bar,
        &MODEL_EXTENSIONS,
    )
}

pub fn download_dataset_repo_sync(
    repo_id: &str,
    revision: Option<String>,
    cache: Option<PathBuf>,
    token: Option<String>,
    progress_bar: bool,
) -> Result<Vec<PathBuf>, hf_hub::api::sync::ApiError> {
    download_repo_sync(
        match revision {
            Some(revision) => Repo::with_revision(repo_id.to_owned(), RepoType::Dataset, revision),
            None => Repo::new(repo_id.to_owned(), RepoType::Dataset),
        },
        cache,
        token,
        progress_bar,
        &DATASET_EXTENSIONS,
    )
}

#[derive(Debug, Clone)]
pub struct HubUploadInfo {
    pub hub_repo: String,
    pub hub_token: String,
}

pub async fn upload_to_hub(
    hub_info: HubUploadInfo,
    local: Vec<PathBuf>,
    step: u64,
    cancellation_token: tokio_util::sync::CancellationToken,
) -> Result<(), UploadError> {
    let HubUploadInfo {
        hub_repo,
        hub_token,
    } = hub_info;

    if cancellation_token.is_cancelled() {
        return Ok(());
    }

    // Collect all safetensors files to upload in a single commit
    let files_to_upload: Vec<_> = local
        .iter()
        .filter(|p| p.extension() == Some("safetensors".as_ref()))
        .map(|path| -> Result<_, UploadError> {
            let file_name = path
                .file_name()
                .ok_or_else(|| UploadError::NotAFile(path.clone()))?
                .to_str()
                .ok_or_else(|| UploadError::InvalidFilename(path.clone()))?
                .to_string();
            Ok((path.clone().into(), file_name))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if files_to_upload.is_empty() {
        info!(repo = hub_repo, "No safetensors files to upload");
        return Ok(());
    }

    let file_names: Vec<_> = files_to_upload
        .iter()
        .map(|(_, name)| name.clone())
        .collect();
    info!(
        repo = hub_repo,
        file_count = files_to_upload.len(),
        "Uploading checkpoint to HuggingFace"
    );

    let api = hf_hub::api::tokio::ApiBuilder::new()
        .with_token(Some(hub_token))
        .build()?;
    let repo = Repo::model(hub_repo.clone());
    let api_repo = api.repo(repo);

    let upload_future =
        api_repo.upload_files(files_to_upload, Some(format!("step {step}")), None, false);

    tokio::select! {
        biased;

        _ = cancellation_token.cancelled() => {
            info!(repo = hub_repo, "Upload to HuggingFace cancelled");
            return Ok(());
        }
        result = upload_future => {
            result.map_err(|e| {
                error!(repo = hub_repo, error = ?e, "Failed to upload files");
                e
            })?;
        }
    }

    info!(repo = hub_repo, files = ?file_names, "Upload to HuggingFace complete");

    Ok(())
}
