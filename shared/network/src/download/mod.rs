mod manager;
mod scheduler;

pub use manager::{
    DownloadComplete, DownloadFailed, DownloadManager, DownloadManagerEvent, DownloadType,
    DownloadUpdate, TransmittableDownload,
};
pub use scheduler::{DownloadSchedulerHandle, ReadyRetry, RetryConfig, RetryQueueResult};
