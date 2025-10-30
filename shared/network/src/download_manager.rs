use crate::{
    ModelRequestType, Networkable,
    p2p_model_sharing::{TransmittableModelConfig, TransmittableModelParameter},
    serialized_distro::TransmittableDistroResult,
};

use anyhow::{Result, anyhow};
use futures_util::StreamExt;
use iroh::{Endpoint, PublicKey};
use iroh_blobs::api::downloader::Downloader;
use iroh_blobs::store::fs::options::GcConfig;
use iroh_blobs::store::mem::{MemStore, Options as MemStoreOptions};
use iroh_blobs::ticket::BlobTicket;
use iroh_blobs::{Hash, api::downloader::DownloadProgressItem};
use iroh_blobs::{
    HashAndFormat,
    api::{Tag, downloader::ContentDiscovery},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::{collections::HashMap, fmt::Debug, marker::PhantomData, sync::Arc, time::Instant};
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, trace};
use tracing::{info, warn};

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
    last_offset: u64,
    total_size: u64,
    r#type: DownloadType,
}

impl Download {
    fn new(blob_ticket: BlobTicket, tag: Tag, download_type: DownloadType) -> Self {
        Self {
            blob_ticket,
            tag,
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

#[derive(Debug)]
pub struct DownloadManager<D: Networkable> {
    _download_type: PhantomData<D>,
    event_receiver: mpsc::UnboundedReceiver<DownloadManagerEvent<D>>,
    event_sender: mpsc::UnboundedSender<DownloadManagerEvent<D>>,
    pub blobs_store: Arc<MemStore>,
    iroh_downloader: Downloader,
}

impl<D: Networkable + Send + 'static> DownloadManager<D> {
    pub fn new(endpoint: &Endpoint) -> Result<Self> {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        trace!("creating blobs store...");
        let store = MemStore::new_with_opts(MemStoreOptions {
            gc_config: Some(GcConfig {
                interval: Duration::from_secs(10),
                add_protected: None,
            }),
        });
        let downloader = Downloader::new(&store, endpoint);
        trace!("blobs store created!");
        Ok(Self {
            _download_type: PhantomData,
            iroh_downloader: downloader,
            event_receiver,
            event_sender: event_sender.clone(),
            blobs_store: Arc::new(store),
        })
    }

    pub fn start_download(
        &mut self,
        providers: impl ContentDiscovery,
        blob_ticket: BlobTicket,
        tag: Tag,
        download_type: DownloadType,
    ) {
        let progress = self.iroh_downloader.download(blob_ticket.hash(), providers);
        let mut download = Download::new(blob_ticket.clone(), tag.clone(), download_type);
        let event_sender = self.event_sender.clone();
        tokio::spawn(async move {
            info!(
                "Starting download for blob: {}",
                download.blob_ticket.hash()
            );
            let progress = progress.stream().await;
            match progress {
                Ok(mut progress) => {
                    while let Some(val) = progress.next().await {
                        let event = Self::handle_download_progress(val, &mut download);
                        event_sender.send(event.unwrap()).unwrap();
                    }
                }
                Err(e) => panic!("Failed to start download: {e}"),
            }
        });
    }

    pub async fn upload_blob(&self, data: Vec<u8>, tag: Tag) -> Result<HashAndFormat> {
        self.blobs_store
            .add_bytes(data)
            .with_named_tag(tag)
            .await
            .map_err(|e| anyhow!("{e}"))
    }

    pub fn read(&mut self, blob_ticket: BlobTicket, tag: Tag, download_type: DownloadType) {
        let event_sender = self.event_sender.clone();
        let blobs = self.blobs_store.blobs().clone();
        let hash = blob_ticket.hash();
        tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Err(err) = blobs.reader(hash).read_to_end(&mut buf).await {
                error!("Failed to read bytes: {err:#}");
                return;
            }
            let size = buf.len();
            debug!(name: "blob_download_finish", hash = %hash.fmt_short(), "downloaded blob {:?}, {} bytes", hash.fmt_short(), size);
            let event = match postcard::from_bytes(&buf) {
                Ok(decoded) => Some(DownloadManagerEvent::Complete(DownloadComplete {
                    data: decoded,
                    from: blob_ticket.node_addr().node_id,
                    hash: blob_ticket.hash(),
                })),
                Err(err) => Some(DownloadManagerEvent::Failed(DownloadFailed {
                    blob_ticket,
                    tag,
                    error: err.into(),
                    download_type: download_type.clone(),
                })),
            };
            if let Some(event) = event {
                let _ = event_sender.send(event);
            }
        });
    }

    pub async fn poll_next(&mut self) -> Option<DownloadManagerEvent<D>> {
        self.event_receiver.recv().await
    }

    fn handle_download_progress(
        result: DownloadProgressItem,
        download: &mut Download,
    ) -> Option<DownloadManagerEvent<D>> {
        let event = match result {
            DownloadProgressItem::TryProvider {
                id: _id,
                request: _request,
            } => Some(DownloadManagerEvent::Update(DownloadUpdate {
                blob_ticket: download.blob_ticket.clone(),
                tag: download.tag.clone(),
                downloaded_size_delta: 0,
                downloaded_size: 0,
                total_size: 0,
                all_done: false,
                download_type: download.r#type.clone(),
            })),
            DownloadProgressItem::Progress(bytes_amount) => {
                Some(DownloadManagerEvent::Update(DownloadUpdate {
                    blob_ticket: download.blob_ticket.clone(),
                    tag: download.tag.clone(),
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
                    tag: download.tag.clone(),
                    downloaded_size_delta: 0,
                    downloaded_size: download.last_offset,
                    total_size: download.total_size,
                    all_done: true,
                    download_type: download.r#type.clone(),
                }))
            }
            DownloadProgressItem::DownloadError => {
                warn!(
                    "Download error, removing it. idx, hash {}, node provider {}: {}",
                    download.blob_ticket.hash(),
                    download.blob_ticket.node_addr().node_id,
                    "Download error"
                );
                Some(DownloadManagerEvent::Failed(DownloadFailed {
                    blob_ticket: download.blob_ticket.clone(),
                    error: anyhow!("Download error"),
                    tag: download.tag.clone(),
                    download_type: download.r#type.clone(),
                }))
            }
            DownloadProgressItem::Error(e) => {
                warn!(
                    "Download error, removing it. idx, hash {}, node provider {}: {}",
                    download.blob_ticket.hash(),
                    download.blob_ticket.node_addr().node_id,
                    e
                );
                Some(DownloadManagerEvent::Failed(DownloadFailed {
                    blob_ticket: download.blob_ticket.clone(),
                    error: e,
                    tag: download.tag.clone(),
                    download_type: download.r#type.clone(),
                }))
            }
            DownloadProgressItem::ProviderFailed {
                id: _id,
                request: _request,
            } => Some(DownloadManagerEvent::Update(DownloadUpdate {
                blob_ticket: download.blob_ticket.clone(),
                tag: download.tag.clone(),
                downloaded_size_delta: 0,
                downloaded_size: download.last_offset,
                total_size: download.total_size,
                all_done: false,
                download_type: download.r#type.clone(),
            })),
        };

        event
    }
}
