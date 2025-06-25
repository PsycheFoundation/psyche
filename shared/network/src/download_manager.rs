use crate::{
    p2p_model_sharing::{TransmittableModelConfig, TransmittableModelParameter},
    serialized_distro::TransmittableDistroResult,
    ModelRequestType, Networkable,
};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use futures_util::future::select_all;
use iroh::{NodeAddr, PublicKey};
use iroh_blobs::{get::db::DownloadProgress, ticket::BlobTicket};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, future::Future, marker::PhantomData, pin::Pin, sync::Arc};
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    task::JoinHandle,
};
use tracing::{error, info, trace, warn};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum TransmittableDownload {
    DistroResult(TransmittableDistroResult),
    ModelParameter(TransmittableModelParameter),
    ModelConfig(TransmittableModelConfig),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DownloadType {
    // Distro result variant with the list of possible peers that we might ask for the blob in case of failure with the original
    DistroResult(Vec<NodeAddr>),
    // Model sharing variant containing the specific type wether be the model config or a paramter
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
    tag: u32,
    download: mpsc::UnboundedReceiver<Result<DownloadProgress>>,
    last_offset: u64,
    total_size: u64,
    r#type: DownloadType,
}

struct ReadingFinishedDownload {
    blob_ticket: BlobTicket,
    tag: u32,
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
        tag: u32,
        download: mpsc::UnboundedReceiver<Result<DownloadProgress>>,
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
    pub tag: u32,
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
    pub tag: u32,
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
        tag: u32,
        progress: mpsc::UnboundedReceiver<Result<DownloadProgress>>,
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
        tag: u32,
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

        #[derive(Debug)]
        enum FutureResult {
            Download(usize, Result<DownloadProgress>),
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
        result: Result<DownloadProgress>,
        index: usize,
    ) -> Option<DownloadManagerEvent<D>> {
        let download = &mut downloads[index];
        let event = match result {
            Ok(progress) => match progress {
                DownloadProgress::InitialState(_) => None,
                DownloadProgress::FoundLocal { size, .. } => {
                    Some(DownloadManagerEvent::Update(DownloadUpdate {
                        blob_ticket: download.blob_ticket.clone(),
                        tag: download.tag,
                        downloaded_size_delta: 0,
                        downloaded_size: size.value(),
                        total_size: size.value(),
                        all_done: false,
                        download_type: download.r#type.clone(),
                    }))
                }
                DownloadProgress::Connected => None,
                DownloadProgress::Found { size, .. } => {
                    download.total_size = size;
                    Some(DownloadManagerEvent::Update(DownloadUpdate {
                        blob_ticket: download.blob_ticket.clone(),
                        tag: download.tag,
                        downloaded_size_delta: 0,
                        downloaded_size: 0,
                        total_size: size,
                        all_done: false,
                        download_type: download.r#type.clone(),
                    }))
                }
                DownloadProgress::FoundHashSeq { .. } => None,
                DownloadProgress::Progress { offset, .. } => {
                    let delta = offset.saturating_sub(download.last_offset);
                    download.last_offset = offset;
                    Some(DownloadManagerEvent::Update(DownloadUpdate {
                        blob_ticket: download.blob_ticket.clone(),
                        tag: download.tag,
                        downloaded_size_delta: delta,
                        downloaded_size: offset,
                        total_size: download.total_size,
                        all_done: false,
                        download_type: download.r#type.clone(),
                    }))
                }
                DownloadProgress::Done { .. } => None,
                DownloadProgress::AllDone(_) => {
                    Some(DownloadManagerEvent::Update(DownloadUpdate {
                        blob_ticket: download.blob_ticket.clone(),
                        tag: download.tag,
                        downloaded_size_delta: 0,
                        downloaded_size: download.total_size,
                        total_size: download.total_size,
                        all_done: true,
                        download_type: download.r#type.clone(),
                    }))
                }
                DownloadProgress::Abort(err) => {
                    Some(DownloadManagerEvent::Failed(DownloadFailed {
                        blob_ticket: download.blob_ticket.clone(),
                        error: err.into(),
                        tag: download.tag,
                        download_type: download.r#type.clone(),
                    }))
                }
            },
            Err(e) => Some(DownloadManagerEvent::Failed(DownloadFailed {
                blob_ticket: download.blob_ticket.clone(),
                error: e,
                tag: download.tag,
                download_type: download.r#type.clone(),
            })),
        };
        match &event {
            Some(DownloadManagerEvent::Update(DownloadUpdate { all_done, .. })) if *all_done => {
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
                    "Download error, removing it. idx {index}, hash {}: {}",
                    blob_ticket.hash(),
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
                    from: downloader.blob_ticket.node_addr().node_id,
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
