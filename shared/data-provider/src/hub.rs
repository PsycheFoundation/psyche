use hf_hub::{
    Cache, Repo, RepoType,
    api::{
        Siblings,
        tokio::{ApiError, CommitError, UploadSource},
    },
};
use std::{path::PathBuf, time::Instant};
use thiserror::Error;
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
        let futures = chunk
            .iter()
            .map(|x| async {
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
            })
            .collect::<Vec<_>>();
        for future in futures {
            ret.push(future.await?);
        }
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

#[derive(Error, Debug)]
pub enum UploadModelError {
    #[error("path {0} is not a file")]
    NotAFile(PathBuf),

    #[error("file {0} doesn't have a valid utf-8 representation")]
    InvalidFilename(PathBuf),

    #[error("failed to connect to HF hub: {0}")]
    HfHub(#[from] ApiError),

    #[error("failed to commit files: {0}")]
    Commit(#[from] CommitError),
}

pub async fn upload_model_repo_async(
    repo_id: String,
    files: Vec<PathBuf>,
    token: String,
    commit_message: Option<String>,
    commit_description: Option<String>,
) -> Result<String, UploadModelError> {
    let api = hf_hub::api::tokio::ApiBuilder::new()
        .with_token(Some(token))
        .build()?;
    let repo = Repo::model(repo_id.clone());
    let api_repo = api.repo(repo);

    let files: Result<Vec<(UploadSource, String)>, _> = files
        .into_iter()
        .map(|path| {
            path.file_name()
                .ok_or(UploadModelError::NotAFile(path.clone()))
                .and_then(|name| {
                    name.to_str()
                        .ok_or(UploadModelError::InvalidFilename(path.clone()))
                        .map(|s| s.to_string())
                })
                .map(|name| (path.into(), name))
        })
        .collect();

    let files = files?;

    let commit_info = match api_repo
        .upload_files(
            files,
            commit_message.clone(),
            commit_description.clone(),
            false,
        )
        .await
    {
        Ok(info) => {
            info!(
                repo = repo_id,
                oid = info.oid,
                "Successfully uploaded files to HuggingFace"
            );
            info
        }
        Err(e) => {
            error!(
                repo = repo_id,
                error = ?e,
                "Failed to upload files to HuggingFace. Full error details: {:#?}",
                e
            );
            return Err(e.into());
        }
    };
    Ok(commit_info.oid)
}
