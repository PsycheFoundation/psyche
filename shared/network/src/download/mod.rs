mod manager;
mod scheduler;

pub use manager::{
    DownloadComplete, DownloadFailed, DownloadManager, DownloadManagerEvent, DownloadRetryInfo,
    DownloadType, DownloadUpdate, MAX_DOWNLOAD_RETRIES, TransmittableDownload,
};
pub use scheduler::{DownloadSchedulerHandle, ReadyRetry};
