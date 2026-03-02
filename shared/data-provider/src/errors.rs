use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UploadError {
    #[error("path {0} is not a file")]
    NotAFile(PathBuf),

    #[error("file {0} doesn't have a valid utf-8 representation")]
    InvalidFilename(PathBuf),

    #[error("failed to send checkpoint notification")]
    SendCheckpoint,

    // Hub-specific errors
    #[error("failed to connect to HF hub: {0}")]
    HfHub(#[from] hf_hub::api::tokio::ApiError),

    #[error("failed to commit files: {0}")]
    Commit(#[from] hf_hub::api::tokio::CommitError),

    // GCS-specific errors
    #[error("GCS authentication failed: {0}")]
    GcsAuth(#[from] google_cloud_storage::client::google_cloud_auth::error::Error),

    #[error("GCS operation failed: {0}")]
    GcsStorage(#[from] google_cloud_storage::http::Error),

    // Common errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("failed to connect to HF hub: {0}")]
    HfHub(#[from] hf_hub::api::tokio::ApiError),

    #[error("GCS authentication failed: {0}")]
    GcsAuth(#[from] google_cloud_storage::client::google_cloud_auth::error::Error),

    #[error("GCS operation failed: {0}")]
    GcsStorage(#[from] google_cloud_storage::http::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
