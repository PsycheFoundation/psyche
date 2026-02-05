use hf_hub::api::tokio::CommitError;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UploadError {
    #[error("path {0} is not a file")]
    NotAFile(PathBuf),

    #[error("file {0} doesn't have a valid utf-8 representation")]
    InvalidFilename(PathBuf),

    #[error("GCS authentication failed: {0}")]
    GcsAuth(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("GCS error: {0}")]
    Gcs(String),

    #[error("HuggingFace Hub API error: {0}")]
    HubApi(#[from] hf_hub::api::tokio::ApiError),

    #[error("HuggingFace Hub commit error: {0}")]
    HubCommit(#[from] CommitError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("GCS authentication failed: {0}")]
    GcsAuth(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("GCS error: {0}")]
    Gcs(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
