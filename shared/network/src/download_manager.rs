use crate::{
    ModelRequestType, Networkable,
    p2p_model_sharing::{TransmittableModelConfig, TransmittableModelParameter},
    serialized_distro::TransmittableDistroResult,
};

use anyhow::{Result, anyhow};
use bytes::Bytes;
use futures_util::future::select_all;
use iroh::PublicKey;
use iroh_blobs::api::Tag;
use iroh_blobs::ticket::BlobTicket;
use iroh_blobs::{Hash, api::downloader::DownloadProgressItem};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap, fmt::Debug, future::Future, marker::PhantomData, pin::Pin, sync::Arc,
    time::Instant,
};
use tokio::{
    sync::{Mutex, mpsc, oneshot},
    task::JoinHandle,
};
use tracing::{error, info, trace, warn};

pub const MAX_DOWNLOAD_RETRIES: usize = 3;

#[derive(Debug, Clone)]
pub struct DownloadRetryInfo {
    pub retries: usize,
    pub retry_time: Option<Instant>,
    pub ticket: BlobTicket,
    pub tag: Tag,
    pub r#type: DownloadType,
}

#[derive(Debug)]
pub enum RetriedDownloadsMessage {
    Insert {
        info: DownloadRetryInfo,
    },
    Remove {
        hash: Hash,
        response: oneshot::Sender<Option<DownloadRetryInfo>>,
    },
    Get {
        hash: Hash,
        response: oneshot::Sender<Option<DownloadRetryInfo>>,
    },
    PendingRetries {
        response: oneshot::Sender<Vec<(Hash, BlobTicket, Tag, DownloadType)>>,
    },
    UpdateTime {
        hash: Hash,
        response: oneshot::Sender<usize>,
    },
}

/// Handler to interact with the retried downloads actor
#[derive(Clone)]
pub struct RetriedDownloadsHandle {
    tx: mpsc::UnboundedSender<RetriedDownloadsMessage>,
}

impl Default for RetriedDownloadsHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl RetriedDownloadsHandle {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn the actor
        tokio::spawn(retried_downloads_actor(rx));

        Self { tx }
    }

    /// Insert a new download to retry
    pub fn insert(&self, info: DownloadRetryInfo) {
        let _ = self.tx.send(RetriedDownloadsMessage::Insert { info });
    }

    /// Remove a download from the retry list
    pub async fn remove(&self, hash: Hash) -> Option<DownloadRetryInfo> {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx
            .send(RetriedDownloadsMessage::Remove {
                hash,
                response: response_tx,
            })
            .is_err()
        {
            return None;
        }

        response_rx.await.unwrap_or(None)
    }

    /// Get a download from the retry list
    pub async fn get(&self, hash: Hash) -> Option<DownloadRetryInfo> {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx
            .send(RetriedDownloadsMessage::Get {
                hash,
                response: response_tx,
            })
            .is_err()
        {
            return None;
        }

        response_rx.await.unwrap_or(None)
    }

    /// Get the retries that are considered pending and have not been retried yet
    pub async fn pending_retries(&self) -> Vec<(Hash, BlobTicket, Tag, DownloadType)> {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx
            .send(RetriedDownloadsMessage::PendingRetries {
                response: response_tx,
            })
            .is_err()
        {
            return Vec::new();
        }

        response_rx.await.unwrap_or_else(|_| Vec::new())
    }

    /// Mark the retry as already being retried marking updating the retry time
    pub async fn update_time(&self, hash: Hash) -> usize {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx
            .send(RetriedDownloadsMessage::UpdateTime {
                hash,
                response: response_tx,
            })
            .is_err()
        {
            return 0;
        }

        response_rx.await.unwrap_or(0)
    }
}

struct RetriedDownloadsActor {
    downloads: HashMap<Hash, DownloadRetryInfo>,
}

impl RetriedDownloadsActor {
    fn new() -> Self {
        Self {
            downloads: HashMap::new(),
        }
    }

    fn handle_message(&mut self, message: RetriedDownloadsMessage) {
        match message {
            RetriedDownloadsMessage::Insert { info } => {
                let hash = info.ticket.hash();
                self.downloads.insert(hash, info);
            }

            RetriedDownloadsMessage::Remove { hash, response } => {
                let removed = self.downloads.remove(&hash);
                let _ = response.send(removed);
            }

            RetriedDownloadsMessage::Get { hash, response } => {
                let info = self.downloads.get(&hash).cloned();
                let _ = response.send(info);
            }

            RetriedDownloadsMessage::PendingRetries { response } => {
                let now = Instant::now();
                let pending: Vec<_> = self
                    .downloads
                    .iter()
                    .filter(|(_, info)| {
                        info.retry_time
                            .map(|retry_time| now >= retry_time)
                            .unwrap_or(false)
                    })
                    .map(|(hash, info)| {
                        (
                            *hash,
                            info.ticket.clone(),
                            info.tag.clone(),
                            info.r#type.clone(),
                        )
                    })
                    .collect();

                let _ = response.send(pending);
            }

            RetriedDownloadsMessage::UpdateTime { hash, response } => {
                let retries = if let Some(info) = self.downloads.get_mut(&hash) {
                    info.retry_time = None; // Mark as being retried now
                    info.retries
                } else {
                    0
                };

                let _ = response.send(retries);
            }
        }
    }
}

async fn retried_downloads_actor(mut rx: mpsc::UnboundedReceiver<RetriedDownloadsMessage>) {
    let mut actor = RetriedDownloadsActor::new();

    while let Some(message) = rx.recv().await {
        actor.handle_message(message);
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TransmittableDownload {
    DistroResult(TransmittableDistroResult),
    ModelParameter(TransmittableModelParameter),
    ModelConfig(TransmittableModelConfig),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DownloadType {
    // Distro result variant with the list of possible peers that we might ask for the blob in case of failure with the original
    DistroResult(Vec<PublicKey>),
    // Model sharing variant containing the specific type wether be the model config or a parameter
    ModelSharing(ModelRequestType),
}

impl DownloadType {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::DistroResult(..) => "distro_result",
            Self::ModelSharing(..) => "model_sharing",
        }
    }
}

#[derive(Debug)]
struct Download {
    blob_ticket: BlobTicket,
    tag: Tag,
    download: mpsc::UnboundedReceiver<Result<DownloadProgressItem>>,
    last_offset: u64,
    total_size: u64,
    r#type: DownloadType,
}

struct ReadingFinishedDownload {
    blob_ticket: BlobTicket,
    tag: Tag,
    download: oneshot::Receiver<Bytes>,
    r#type: DownloadType,
}

impl Debug for ReadingFinishedDownload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadingFinishedDownload")
            .field("blob_ticket", &self.blob_ticket)
            .field("reading", &"...")
            .finish()
    }
}

impl Download {
    fn new(
        blob_ticket: BlobTicket,
        tag: Tag,
        download: mpsc::UnboundedReceiver<Result<DownloadProgressItem>>,
        download_type: DownloadType,
    ) -> Self {
        Self {
            blob_ticket,
            tag,
            download,
            last_offset: 0,
            total_size: 0,
            r#type: download_type,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadUpdate {
    pub blob_ticket: BlobTicket,
    pub tag: Tag,
    pub downloaded_size_delta: u64,
    pub downloaded_size: u64,
    pub total_size: u64,
    pub all_done: bool,
    pub download_type: DownloadType,
}

pub struct DownloadComplete<D: Networkable> {
    pub hash: iroh_blobs::Hash,
    pub from: PublicKey,
    pub data: D,
}

#[derive(Debug)]
pub struct DownloadFailed {
    pub blob_ticket: BlobTicket,
    pub tag: Tag,
    pub error: anyhow::Error,
    pub download_type: DownloadType,
}

impl<D: Networkable> Debug for DownloadComplete<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadComplete")
            .field("hash", &self.hash)
            .field("from", &self.from)
            .field("data", &"...")
            .finish()
    }
}

pub enum DownloadManagerEvent<D: Networkable> {
    Update(DownloadUpdate),
    Complete(DownloadComplete<D>),
    Failed(DownloadFailed),
}

impl<D: Networkable> Debug for DownloadManagerEvent<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Update(arg0) => f.debug_tuple("Update").field(arg0).finish(),
            Self::Complete(arg0) => f.debug_tuple("Complete").field(arg0).finish(),
            Self::Failed(arg0) => f.debug_tuple("Failed").field(arg0).finish(),
        }
    }
}

pub struct DownloadManager<D: Networkable> {
    downloads: Arc<Mutex<Vec<Download>>>,
    reading: Arc<Mutex<Vec<ReadingFinishedDownload>>>,
    _download_type: PhantomData<D>,
    task_handle: Option<JoinHandle<()>>,
    event_receiver: mpsc::UnboundedReceiver<DownloadManagerEvent<D>>,
    tx_new_item: mpsc::UnboundedSender<()>,
}

impl<D: Networkable> Debug for DownloadManager<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadManager")
            .field("downloads", &self.downloads)
            .field("reading", &self.reading)
            .finish()
    }
}

impl<D: Networkable + Send + 'static> DownloadManager<D> {
    pub fn new() -> Result<Self> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let (tx_new_item, mut rx_new_item) = mpsc::unbounded_channel();

        let downloads = Arc::new(Mutex::new(Vec::new()));
        let reading = Arc::new(Mutex::new(Vec::new()));
        let mut manager = Self {
            downloads: downloads.clone(),
            reading: reading.clone(),
            _download_type: PhantomData,
            task_handle: None,
            event_receiver,
            tx_new_item,
        };

        let task_handle = tokio::spawn(async move {
            loop {
                if downloads.lock().await.is_empty()
                    && reading.lock().await.is_empty()
                    && rx_new_item.recv().await.is_none()
                {
                    // channel is closed.
                    info!("Download manager channel closed - shutting down.");
                    return;
                }

                if let Some(event) =
                    Self::poll_next_inner(&mut *downloads.lock().await, &mut *reading.lock().await)
                        .await
                {
                    if event_sender.send(event).is_err() {
                        warn!("Event sender in download manager closed.");
                        break;
                    }
                }
            }
        });

        manager.task_handle = Some(task_handle);

        Ok(manager)
    }

    pub fn add(
        &mut self,
        blob_ticket: BlobTicket,
        tag: Tag,
        progress: mpsc::UnboundedReceiver<Result<DownloadProgressItem>>,
        download_type: DownloadType,
    ) {
        let downloads = self.downloads.clone();
        let sender = self.tx_new_item.clone();
        tokio::spawn(async move {
            downloads
                .lock()
                .await
                .push(Download::new(blob_ticket, tag, progress, download_type));

            if let Err(err) = sender.send(()) {
                error!("{err:#}");
            }
        });
    }

    pub fn read(
        &mut self,
        blob_ticket: BlobTicket,
        tag: Tag,
        download: oneshot::Receiver<Bytes>,
        download_type: DownloadType,
    ) {
        let reading = self.reading.clone();
        let sender = self.tx_new_item.clone();
        tokio::spawn(async move {
            reading.lock().await.push(ReadingFinishedDownload {
                blob_ticket,
                tag,
                download,
                r#type: download_type,
            });
            if let Err(err) = sender.send(()) {
                error!("{err:#}");
            }
        });
    }

    pub async fn poll_next(&mut self) -> Option<DownloadManagerEvent<D>> {
        self.event_receiver.recv().await
    }

    async fn poll_next_inner(
        downloads: &mut Vec<Download>,
        reading: &mut Vec<ReadingFinishedDownload>,
    ) -> Option<DownloadManagerEvent<D>> {
        if downloads.is_empty() && reading.is_empty() {
            return None;
        }

        enum FutureResult {
            Download(usize, Result<DownloadProgressItem>),
            Read(usize, Result<Bytes>),
        }

        let download_futures = downloads.iter_mut().enumerate().map(|(i, download)| {
            Box::pin(async move {
                FutureResult::Download(
                    i,
                    download.download.recv().await.unwrap_or_else(|| {
                        Err(anyhow!(
                            "download channel closed when trying to download blob with hash {}.",
                            download.blob_ticket.hash()
                        ))
                    }),
                )
            }) as Pin<Box<dyn Future<Output = FutureResult> + Send>>
        });

        let read_futures = reading.iter_mut().enumerate().map(|(i, read)| {
            Box::pin(async move {
                FutureResult::Read(i, (&mut read.download).await.map_err(|e| e.into()))
            }) as Pin<Box<dyn Future<Output = FutureResult> + Send>>
        });

        let all_futures: Vec<Pin<Box<dyn Future<Output = FutureResult> + Send>>> =
            download_futures.chain(read_futures).collect();

        let result = select_all(all_futures).await.0;

        match result {
            FutureResult::Download(index, result) => {
                Self::handle_download_progress(downloads, result, index)
            }
            FutureResult::Read(index, result) => {
                let downloader: ReadingFinishedDownload = reading.swap_remove(index);
                tokio::task::spawn_blocking(move || Self::handle_read_result(downloader, result))
                    .await
                    .unwrap()
            }
        }
    }

    fn handle_download_progress(
        downloads: &mut Vec<Download>,
        result: Result<DownloadProgressItem>,
        index: usize,
    ) -> Option<DownloadManagerEvent<D>> {
        let download = &mut downloads[index];
        let tag = download.tag.clone();
        let event = match result {
            Ok(progress) => match progress {
                DownloadProgressItem::TryProvider {
                    id: _id,
                    request: _request,
                } => Some(DownloadManagerEvent::Update(DownloadUpdate {
                    blob_ticket: download.blob_ticket.clone(),
                    tag,
                    downloaded_size_delta: 0,
                    downloaded_size: 0,
                    total_size: 0,
                    all_done: false,
                    download_type: download.r#type.clone(),
                })),
                DownloadProgressItem::Progress(bytes_amount) => {
                    Some(DownloadManagerEvent::Update(DownloadUpdate {
                        blob_ticket: download.blob_ticket.clone(),
                        tag,
                        downloaded_size_delta: bytes_amount.saturating_sub(download.last_offset),
                        downloaded_size: bytes_amount,
                        total_size: download.total_size,
                        all_done: false,
                        download_type: download.r#type.clone(),
                    }))
                }
                // We're using the Blob format so there's only one part for each blob
                DownloadProgressItem::PartComplete { request: _request } => {
                    Some(DownloadManagerEvent::Update(DownloadUpdate {
                        blob_ticket: download.blob_ticket.clone(),
                        tag,
                        downloaded_size_delta: 0,
                        downloaded_size: download.last_offset,
                        total_size: download.total_size,
                        all_done: true,
                        download_type: download.r#type.clone(),
                    }))
                }
                DownloadProgressItem::DownloadError => {
                    Some(DownloadManagerEvent::Failed(DownloadFailed {
                        blob_ticket: download.blob_ticket.clone(),
                        error: anyhow!("Download error"),
                        tag,
                        download_type: download.r#type.clone(),
                    }))
                }
                DownloadProgressItem::Error(e) => {
                    Some(DownloadManagerEvent::Failed(DownloadFailed {
                        blob_ticket: download.blob_ticket.clone(),
                        error: e.into(),
                        tag,
                        download_type: download.r#type.clone(),
                    }))
                }
                DownloadProgressItem::ProviderFailed {
                    id: _id,
                    request: _request,
                } => Some(DownloadManagerEvent::Update(DownloadUpdate {
                    blob_ticket: download.blob_ticket.clone(),
                    tag,
                    downloaded_size_delta: 0,
                    downloaded_size: download.last_offset,
                    total_size: download.total_size,
                    all_done: false,
                    download_type: download.r#type.clone(),
                })),
            },
            Err(err) => Some(DownloadManagerEvent::Failed(DownloadFailed {
                blob_ticket: download.blob_ticket.clone(),
                error: err,
                tag,
                download_type: download.r#type.clone(),
            })),
        };
        match &event {
            Some(DownloadManagerEvent::Update(DownloadUpdate {
                all_done,
                downloaded_size,
                ..
            })) if *all_done => {
                download.last_offset = *downloaded_size;
                let removed = downloads.swap_remove(index);
                trace!(
                    "Since download is complete, removing it: idx {index}, hash {}",
                    removed.blob_ticket.hash()
                );
            }
            Some(DownloadManagerEvent::Failed(DownloadFailed {
                blob_ticket, error, ..
            })) => {
                downloads.swap_remove(index);
                warn!(
                    "Download error, removing it. idx {index}, hash {}, node provider {}: {}",
                    blob_ticket.hash(),
                    blob_ticket.addr().id,
                    error
                );
            }
            _ => {
                // download update is normal, doesn't cause removal.
            }
        }
        event
    }

    fn handle_read_result(
        downloader: ReadingFinishedDownload,
        result: Result<Bytes>,
    ) -> Option<DownloadManagerEvent<D>> {
        match result {
            Ok(bytes) => match postcard::from_bytes(&bytes) {
                Ok(decoded) => Some(DownloadManagerEvent::Complete(DownloadComplete {
                    data: decoded,
                    from: downloader.blob_ticket.addr().id,
                    hash: downloader.blob_ticket.hash(),
                })),
                Err(err) => Some(DownloadManagerEvent::Failed(DownloadFailed {
                    blob_ticket: downloader.blob_ticket,
                    tag: downloader.tag,
                    error: err.into(),
                    download_type: downloader.r#type.clone(),
                })),
            },
            Err(e) => Some(DownloadManagerEvent::Failed(DownloadFailed {
                blob_ticket: downloader.blob_ticket,
                tag: downloader.tag,
                error: e,
                download_type: downloader.r#type.clone(),
            })),
        }
    }
}
