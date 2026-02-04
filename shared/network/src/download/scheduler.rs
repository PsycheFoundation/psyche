use std::{collections::HashMap, time::Instant};

use iroh_blobs::{Hash, api::Tag, ticket::BlobTicket};
use tokio::sync::{mpsc, oneshot};
use tracing::info;

use super::manager::{DownloadRetryInfo, DownloadType};
use crate::ModelRequestType;

/// A retry that is ready to be started.
#[derive(Debug, Clone)]
pub struct ReadyRetry {
    pub hash: Hash,
    pub ticket: BlobTicket,
    pub tag: Tag,
    pub download_type: DownloadType,
    pub retries: usize,
}

/// Messages for the download scheduler actor.
///
/// The scheduler manages:
/// 1. Rate limiting for Parameter downloads (concurrency control)
/// 2. Retry tracking for all download types (Parameter and DistroResult)
#[derive(Debug)]
pub enum SchedulerMessage {
    /// Wait for capacity to start a new Parameter download.
    /// When granted, the active count is incremented automatically.
    /// Only used for Parameter downloads, not DistroResult.
    WaitForCapacity { response: oneshot::Sender<()> },

    /// Release a download slot. Called when a Parameter download completes or fails.
    /// This decrements the active count and notifies any waiters.
    ReleaseCapacity,

    /// Queue a failed download for retry with backoff.
    /// Works for both Parameter and DistroResult downloads.
    QueueRetry { info: DownloadRetryInfo },

    /// Remove a download from the retry queue (e.g., on successful completion).
    RemoveRetry {
        hash: Hash,
        response: oneshot::Sender<Option<DownloadRetryInfo>>,
    },

    /// Get retry info for a specific hash (to check retry count).
    GetRetryInfo {
        hash: Hash,
        response: oneshot::Sender<Option<DownloadRetryInfo>>,
    },

    /// Try to start a due Parameter retry if capacity is available.
    /// If successful, returns the retry info and reserves the slot.
    /// If no capacity or no due retries, returns None.
    TryStartParameterRetry {
        response: oneshot::Sender<Option<ReadyRetry>>,
    },

    /// Get all due DistroResult retries (no capacity check needed).
    /// These are removed from the retry queue.
    GetDueDistroRetries {
        response: oneshot::Sender<Vec<ReadyRetry>>,
    },
}

/// Handle to communicate with the download scheduler actor.
#[derive(Clone)]
pub struct DownloadSchedulerHandle {
    tx: mpsc::UnboundedSender<SchedulerMessage>,
}

impl DownloadSchedulerHandle {
    /// Create a new download scheduler with the specified concurrency limit.
    pub fn new(max_concurrent_downloads: usize) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(download_scheduler_actor(rx, max_concurrent_downloads));

        Self { tx }
    }

    /// Wait for capacity to start a new download.
    /// This will block until a slot is available.
    /// When this returns, the slot has been reserved (active count incremented).
    pub async fn wait_for_capacity(&self) {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx
            .send(SchedulerMessage::WaitForCapacity {
                response: response_tx,
            })
            .is_err()
        {
            return;
        }

        let _ = response_rx.await;
    }

    /// Release a download slot. Call this when a download completes or fails.
    pub fn release_capacity(&self) {
        let _ = self.tx.send(SchedulerMessage::ReleaseCapacity);
    }

    /// Queue a failed download for retry.
    pub fn queue_retry(&self, info: DownloadRetryInfo) {
        let _ = self.tx.send(SchedulerMessage::QueueRetry { info });
    }

    /// Remove a download from the retry queue.
    pub async fn remove_retry(&self, hash: Hash) -> Option<DownloadRetryInfo> {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx
            .send(SchedulerMessage::RemoveRetry {
                hash,
                response: response_tx,
            })
            .is_err()
        {
            return None;
        }

        response_rx.await.unwrap_or(None)
    }

    /// Get retry info for a specific hash.
    pub async fn get_retry_info(&self, hash: Hash) -> Option<DownloadRetryInfo> {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx
            .send(SchedulerMessage::GetRetryInfo {
                hash,
                response: response_tx,
            })
            .is_err()
        {
            return None;
        }

        response_rx.await.unwrap_or(None)
    }

    /// Try to start a due Parameter retry if capacity is available.
    /// If successful, the slot is reserved and the retry info is returned.
    /// The retry is removed from the pending queue.
    pub async fn try_start_parameter_retry(&self) -> Option<ReadyRetry> {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx
            .send(SchedulerMessage::TryStartParameterRetry {
                response: response_tx,
            })
            .is_err()
        {
            return None;
        }

        response_rx.await.unwrap_or(None)
    }

    /// Get all due DistroResult retries (no capacity check).
    /// These are removed from the retry queue.
    pub async fn get_due_distro_retries(&self) -> Vec<ReadyRetry> {
        let (response_tx, response_rx) = oneshot::channel();

        if self
            .tx
            .send(SchedulerMessage::GetDueDistroRetries {
                response: response_tx,
            })
            .is_err()
        {
            return Vec::new();
        }

        response_rx.await.unwrap_or_default()
    }
}

/// Internal actor state.
struct DownloadSchedulerActor {
    /// Current number of active downloads.
    active_downloads: usize,
    /// Maximum concurrent downloads allowed.
    max_concurrent: usize,
    /// Queue of callers waiting for capacity.
    waiting_for_capacity: Vec<oneshot::Sender<()>>,
    /// Downloads pending retry, keyed by hash.
    pending_retries: HashMap<Hash, DownloadRetryInfo>,
}

impl DownloadSchedulerActor {
    fn new(max_concurrent: usize) -> Self {
        Self {
            active_downloads: 0,
            max_concurrent,
            waiting_for_capacity: Vec::new(),
            pending_retries: HashMap::new(),
        }
    }

    fn handle_message(&mut self, message: SchedulerMessage) {
        match message {
            SchedulerMessage::WaitForCapacity { response } => {
                if self.active_downloads < self.max_concurrent {
                    // Capacity available, grant immediately
                    self.active_downloads += 1;
                    let _ = response.send(());
                } else {
                    // At capacity, queue the waiter
                    self.waiting_for_capacity.push(response);
                }
            }

            SchedulerMessage::ReleaseCapacity => {
                self.active_downloads = self.active_downloads.saturating_sub(1);
                self.notify_next_waiter();
            }

            SchedulerMessage::QueueRetry { info } => {
                let hash = info.ticket.hash();
                self.pending_retries.insert(hash, info);
            }

            SchedulerMessage::RemoveRetry { hash, response } => {
                let removed = self.pending_retries.remove(&hash);
                let _ = response.send(removed);
            }

            SchedulerMessage::GetRetryInfo { hash, response } => {
                let info = self.pending_retries.get(&hash).cloned();
                let _ = response.send(info);
            }

            SchedulerMessage::TryStartParameterRetry { response } => {
                // Only proceed if we have capacity
                if self.active_downloads >= self.max_concurrent {
                    let _ = response.send(None);
                    return;
                }

                // Find a Parameter retry that is due
                let now = Instant::now();
                let due_retry = self
                    .pending_retries
                    .iter()
                    .find(|(_, info)| {
                        matches!(
                            &info.r#type,
                            DownloadType::ModelSharing(ModelRequestType::Parameter(_))
                        ) && info
                            .retry_time
                            .map(|retry_time| now >= retry_time)
                            .unwrap_or(false)
                    })
                    .map(|(hash, info)| (*hash, info.clone()));

                if let Some((hash, info)) = due_retry {
                    // Reserve the slot
                    self.active_downloads += 1;
                    // Remove from pending retries
                    self.pending_retries.remove(&hash);

                    let ready = ReadyRetry {
                        hash,
                        ticket: info.ticket,
                        tag: info.tag,
                        download_type: info.r#type,
                        retries: info.retries,
                    };
                    let _ = response.send(Some(ready));
                } else {
                    let _ = response.send(None);
                }
            }

            SchedulerMessage::GetDueDistroRetries { response } => {
                let now = Instant::now();

                // Find all due DistroResult retries
                let due_hashes: Vec<Hash> = self
                    .pending_retries
                    .iter()
                    .filter(|(_, info)| {
                        matches!(&info.r#type, DownloadType::DistroResult(_))
                            && info
                                .retry_time
                                .map(|retry_time| now >= retry_time)
                                .unwrap_or(false)
                    })
                    .map(|(hash, _)| *hash)
                    .collect();

                // Remove and collect them
                let ready_retries: Vec<ReadyRetry> = due_hashes
                    .into_iter()
                    .filter_map(|hash| {
                        self.pending_retries.remove(&hash).map(|info| ReadyRetry {
                            hash,
                            ticket: info.ticket,
                            tag: info.tag,
                            download_type: info.r#type,
                            retries: info.retries,
                        })
                    })
                    .collect();

                let _ = response.send(ready_retries);
            }
        }
    }

    /// Notify the next waiter in queue that capacity is available.
    fn notify_next_waiter(&mut self) {
        // Try to find a waiter whose channel is still open
        while let Some(waiter) = self.waiting_for_capacity.pop() {
            if waiter.send(()).is_ok() {
                // Successfully notified, reserve the slot
                self.active_downloads += 1;
                info!(
                    "Granted capacity to waiting requester ({}/{} active)",
                    self.active_downloads, self.max_concurrent
                );
                return;
            }
            // Channel closed, try next waiter
        }
    }
}

async fn download_scheduler_actor(
    mut rx: mpsc::UnboundedReceiver<SchedulerMessage>,
    max_concurrent: usize,
) {
    let mut actor = DownloadSchedulerActor::new(max_concurrent);

    while let Some(message) = rx.recv().await {
        actor.handle_message(message);
    }
}
