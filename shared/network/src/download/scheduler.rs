use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use iroh_blobs::{Hash, api::Tag, ticket::BlobTicket};
use tokio::sync::{mpsc, oneshot};
use tracing::info;

use super::manager::DownloadType;
use crate::ModelRequestType;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub backoff_base: Duration,
    pub max_distro_retries: usize,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            backoff_base: Duration::from_secs(2),
            max_distro_retries: 3,
        }
    }
}

struct RetryEntry {
    retries: usize,
    retry_time: Option<Instant>,
    ticket: BlobTicket,
    tag: Tag,
    download_type: DownloadType,
}

impl RetryEntry {
    fn into_ready_retry(self, hash: Hash) -> ReadyRetry {
        ReadyRetry {
            hash,
            ticket: self.ticket,
            tag: self.tag,
            download_type: self.download_type,
            retries: self.retries,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryQueueResult {
    Queued,
    MaxRetriesExceeded,
}

#[derive(Debug, Clone)]
pub struct ReadyRetry {
    pub hash: Hash,
    pub ticket: BlobTicket,
    pub tag: Tag,
    pub download_type: DownloadType,
    pub retries: usize,
}

#[derive(Debug)]
enum SchedulerMessage {
    WaitForCapacity {
        response: oneshot::Sender<()>,
    },
    ReleaseCapacity,
    QueueFailedDownload {
        ticket: BlobTicket,
        tag: Tag,
        download_type: DownloadType,
        response: oneshot::Sender<RetryQueueResult>,
    },
    RemoveRetry {
        hash: Hash,
        response: oneshot::Sender<bool>,
    },
    StartParameterRetry {
        response: oneshot::Sender<Option<ReadyRetry>>,
    },
    GetDueConfigRetries {
        response: oneshot::Sender<Vec<ReadyRetry>>,
    },
    GetDueDistroRetries {
        response: oneshot::Sender<Vec<ReadyRetry>>,
    },
}

#[derive(Clone)]
pub struct DownloadSchedulerHandle {
    tx: mpsc::UnboundedSender<SchedulerMessage>,
}

impl DownloadSchedulerHandle {
    pub fn new(max_concurrent_downloads: usize, retry_config: RetryConfig) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(download_scheduler_actor(
            rx,
            max_concurrent_downloads,
            retry_config,
        ));

        Self { tx }
    }

    /// Send a message to the actor and await the response via oneshot.
    /// Returns `default` if the actor has shut down or the response channel is dropped.
    async fn request<T>(
        &self,
        make_msg: impl FnOnce(oneshot::Sender<T>) -> SchedulerMessage,
        default: T,
    ) -> T {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(make_msg(tx)).is_err() {
            return default;
        }
        rx.await.unwrap_or(default)
    }

    pub async fn wait_for_capacity(&self) -> anyhow::Result<()> {
        let (response_tx, response_rx) = oneshot::channel();
        self.tx
            .send(SchedulerMessage::WaitForCapacity {
                response: response_tx,
            })
            .map_err(|_| anyhow::anyhow!("Download scheduler actor has shut down"))?;
        response_rx
            .await
            .map_err(|_| anyhow::anyhow!("Download scheduler actor dropped before responding"))
    }

    pub fn release_capacity(&self) {
        let _ = self.tx.send(SchedulerMessage::ReleaseCapacity);
    }

    pub async fn queue_failed_download(
        &self,
        ticket: BlobTicket,
        tag: Tag,
        download_type: DownloadType,
    ) -> RetryQueueResult {
        self.request(
            |response| SchedulerMessage::QueueFailedDownload {
                ticket,
                tag,
                download_type,
                response,
            },
            RetryQueueResult::MaxRetriesExceeded,
        )
        .await
    }

    pub async fn remove_retry(&self, hash: Hash) -> bool {
        self.request(
            |response| SchedulerMessage::RemoveRetry { hash, response },
            false,
        )
        .await
    }

    pub async fn start_parameter_retry(&self) -> Option<ReadyRetry> {
        self.request(
            |response| SchedulerMessage::StartParameterRetry { response },
            None,
        )
        .await
    }

    pub async fn get_due_config_retries(&self) -> Vec<ReadyRetry> {
        self.request(
            |response| SchedulerMessage::GetDueConfigRetries { response },
            Vec::new(),
        )
        .await
    }

    pub async fn get_due_distro_retries(&self) -> Vec<ReadyRetry> {
        self.request(
            |response| SchedulerMessage::GetDueDistroRetries { response },
            Vec::new(),
        )
        .await
    }
}

struct DownloadSchedulerActor {
    active_downloads: usize,
    max_concurrent: usize,
    waiting_for_capacity: VecDeque<oneshot::Sender<()>>,
    retry_entries: HashMap<Hash, RetryEntry>,
    retry_config: RetryConfig,
}

impl DownloadSchedulerActor {
    fn new(max_concurrent: usize, retry_config: RetryConfig) -> Self {
        Self {
            active_downloads: 0,
            max_concurrent,
            waiting_for_capacity: VecDeque::new(),
            retry_entries: HashMap::new(),
            retry_config,
        }
    }

    fn handle_message(&mut self, message: SchedulerMessage) {
        match message {
            SchedulerMessage::WaitForCapacity { response } => {
                if self.active_downloads < self.max_concurrent {
                    self.active_downloads += 1;
                    let _ = response.send(());
                } else {
                    self.waiting_for_capacity.push_back(response);
                }
            }

            SchedulerMessage::ReleaseCapacity => {
                self.active_downloads = self.active_downloads.saturating_sub(1);
                self.notify_next_waiter();
            }

            SchedulerMessage::QueueFailedDownload {
                ticket,
                tag,
                download_type,
                response,
            } => {
                let hash = ticket.hash();
                let prev_retries = self
                    .retry_entries
                    .get(&hash)
                    .map(|e| e.retries)
                    .unwrap_or(0);

                match &download_type {
                    DownloadType::ModelSharing(_) => {
                        self.retry_entries.insert(
                            hash,
                            RetryEntry {
                                retries: prev_retries + 1,
                                retry_time: None,
                                ticket,
                                tag,
                                download_type,
                            },
                        );
                        let _ = response.send(RetryQueueResult::Queued);
                    }
                    DownloadType::DistroResult(_) => {
                        let new_retries = prev_retries + 1;
                        if new_retries > self.retry_config.max_distro_retries {
                            self.retry_entries.remove(&hash);
                            let _ = response.send(RetryQueueResult::MaxRetriesExceeded);
                        } else {
                            let backoff = self
                                .retry_config
                                .backoff_base
                                .mul_f32(2_f32.powi(prev_retries as i32));
                            self.retry_entries.insert(
                                hash,
                                RetryEntry {
                                    retries: new_retries,
                                    retry_time: Some(Instant::now() + backoff),
                                    ticket,
                                    tag,
                                    download_type,
                                },
                            );
                            let _ = response.send(RetryQueueResult::Queued);
                        }
                    }
                }
            }

            SchedulerMessage::RemoveRetry { hash, response } => {
                let removed = self.retry_entries.remove(&hash).is_some();
                let _ = response.send(removed);
            }

            SchedulerMessage::StartParameterRetry { response } => {
                if self.active_downloads >= self.max_concurrent {
                    let _ = response.send(None);
                    return;
                }

                let now = Instant::now();
                let due_entry = self
                    .retry_entries
                    .iter()
                    .find(|(_, entry)| {
                        matches!(
                            &entry.download_type,
                            DownloadType::ModelSharing(ModelRequestType::Parameter(_))
                        ) && entry
                            .retry_time
                            .map(|retry_time| now >= retry_time)
                            .unwrap_or(true)
                    })
                    .map(|(hash, _)| *hash);

                if let Some(hash) = due_entry {
                    self.active_downloads += 1;
                    let entry = self.retry_entries.remove(&hash).unwrap();
                    let _ = response.send(Some(entry.into_ready_retry(hash)));
                } else {
                    let _ = response.send(None);
                }
            }

            SchedulerMessage::GetDueConfigRetries { response } => {
                let retries = self.drain_retries(|entry| {
                    matches!(
                        &entry.download_type,
                        DownloadType::ModelSharing(ModelRequestType::Config)
                    )
                });
                let _ = response.send(retries);
            }

            SchedulerMessage::GetDueDistroRetries { response } => {
                let now = Instant::now();
                let retries = self.drain_retries(|entry| {
                    matches!(&entry.download_type, DownloadType::DistroResult(_))
                        && entry.retry_time.map(|t| now >= t).unwrap_or(false)
                });
                let _ = response.send(retries);
            }
        }
    }

    fn drain_retries(&mut self, predicate: impl Fn(&RetryEntry) -> bool) -> Vec<ReadyRetry> {
        let due_hashes: Vec<Hash> = self
            .retry_entries
            .iter()
            .filter(|(_, entry)| predicate(entry))
            .map(|(hash, _)| *hash)
            .collect();

        due_hashes
            .into_iter()
            .filter_map(|hash| {
                self.retry_entries
                    .remove(&hash)
                    .map(|entry| entry.into_ready_retry(hash))
            })
            .collect()
    }

    fn notify_next_waiter(&mut self) {
        while let Some(waiter) = self.waiting_for_capacity.pop_front() {
            if waiter.send(()).is_ok() {
                self.active_downloads += 1;
                info!(
                    "Granted capacity to waiting requester ({}/{} active)",
                    self.active_downloads, self.max_concurrent
                );
                return;
            }
        }
    }
}

async fn download_scheduler_actor(
    mut rx: mpsc::UnboundedReceiver<SchedulerMessage>,
    max_concurrent: usize,
    retry_config: RetryConfig,
) {
    let mut actor = DownloadSchedulerActor::new(max_concurrent, retry_config);

    while let Some(message) = rx.recv().await {
        actor.handle_message(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelRequestType;
    use iroh::{EndpointAddr, SecretKey};
    use iroh_blobs::BlobFormat;
    use std::time::Duration;

    fn fast_config() -> RetryConfig {
        RetryConfig {
            backoff_base: Duration::from_millis(10),
            max_distro_retries: 3,
        }
    }

    fn dummy_ticket(seed: u8) -> BlobTicket {
        let key = SecretKey::from_bytes(&[seed; 32]);
        let addr = EndpointAddr::from(key.public());
        let hash = Hash::new(&[seed]);
        BlobTicket::new(addr, hash, BlobFormat::Raw)
    }

    fn param_download_type(seed: u8) -> DownloadType {
        DownloadType::ModelSharing(ModelRequestType::Parameter(format!("layer.{seed}")))
    }

    fn config_download_type() -> DownloadType {
        DownloadType::ModelSharing(ModelRequestType::Config)
    }

    fn distro_download_type() -> DownloadType {
        DownloadType::DistroResult(vec![])
    }

    #[tokio::test]
    async fn test_capacity_grants_up_to_max() {
        let scheduler = DownloadSchedulerHandle::new(2, RetryConfig::default());

        scheduler.wait_for_capacity().await.unwrap();
        scheduler.wait_for_capacity().await.unwrap();

        let result =
            tokio::time::timeout(Duration::from_millis(50), scheduler.wait_for_capacity()).await;
        assert!(result.is_err(), "Third request should have timed out");
    }

    #[tokio::test]
    async fn test_release_unblocks_waiter() {
        let scheduler = DownloadSchedulerHandle::new(1, RetryConfig::default());
        scheduler.wait_for_capacity().await.unwrap();

        let scheduler_clone = scheduler.clone();
        let waiter = tokio::spawn(async move { scheduler_clone.wait_for_capacity().await });
        tokio::time::sleep(Duration::from_millis(10)).await;

        scheduler.release_capacity();

        let result = tokio::time::timeout(Duration::from_millis(100), waiter).await;
        assert!(result.is_ok(), "Waiter should have been unblocked");
    }

    #[tokio::test]
    async fn test_waiters_are_served_fifo() {
        let scheduler = DownloadSchedulerHandle::new(1, RetryConfig::default());
        scheduler.wait_for_capacity().await.unwrap();

        let scheduler1 = scheduler.clone();
        let scheduler2 = scheduler.clone();
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let order1 = order.clone();
        let order2 = order.clone();

        let w1 = tokio::spawn(async move {
            scheduler1.wait_for_capacity().await.unwrap();
            order1.lock().unwrap().push(1);
            tokio::time::sleep(Duration::from_millis(5)).await;
            scheduler1.release_capacity();
        });
        tokio::time::sleep(Duration::from_millis(10)).await;

        let w2 = tokio::spawn(async move {
            scheduler2.wait_for_capacity().await.unwrap();
            order2.lock().unwrap().push(2);
            scheduler2.release_capacity();
        });

        scheduler.release_capacity();

        tokio::time::timeout(Duration::from_millis(200), async {
            w1.await.unwrap();
            w2.await.unwrap();
        })
        .await
        .unwrap();

        assert_eq!(
            *order.lock().unwrap(),
            vec![1, 2],
            "Waiters should be served FIFO"
        );
    }

    #[tokio::test]
    async fn test_model_sharing_retry_respects_capacity() {
        let scheduler = DownloadSchedulerHandle::new(1, RetryConfig::default());
        scheduler.wait_for_capacity().await.unwrap();

        let result = scheduler
            .queue_failed_download(
                dummy_ticket(1),
                Tag::from("param-1"),
                param_download_type(1),
            )
            .await;
        assert_eq!(result, RetryQueueResult::Queued);

        assert!(
            scheduler.start_parameter_retry().await.is_none(),
            "Should not return retry when at capacity"
        );

        scheduler.release_capacity();
        tokio::task::yield_now().await;

        assert!(
            scheduler.start_parameter_retry().await.is_some(),
            "Should return retry when capacity is available"
        );
    }

    #[tokio::test]
    async fn test_parameter_retries_are_immediate() {
        let scheduler = DownloadSchedulerHandle::new(2, RetryConfig::default());

        scheduler
            .queue_failed_download(
                dummy_ticket(1),
                Tag::from("param-1"),
                param_download_type(1),
            )
            .await;
        assert!(scheduler.start_parameter_retry().await.is_some());
    }

    #[tokio::test]
    async fn test_config_retries_dont_consume_capacity() {
        let scheduler = DownloadSchedulerHandle::new(1, RetryConfig::default());

        // Fill capacity
        scheduler.wait_for_capacity().await.unwrap();

        // Config retry should still be returned even at full capacity
        scheduler
            .queue_failed_download(dummy_ticket(2), Tag::from("config"), config_download_type())
            .await;
        let retries = scheduler.get_due_config_retries().await;
        assert_eq!(retries.len(), 1);
        assert!(matches!(
            retries[0].download_type,
            DownloadType::ModelSharing(ModelRequestType::Config)
        ));
    }

    #[tokio::test]
    async fn test_distro_retry_not_immediately_due() {
        let scheduler = DownloadSchedulerHandle::new(2, fast_config());

        let result = scheduler
            .queue_failed_download(
                dummy_ticket(1),
                Tag::from("distro-1"),
                distro_download_type(),
            )
            .await;
        assert_eq!(result, RetryQueueResult::Queued);

        assert!(
            scheduler.get_due_distro_retries().await.is_empty(),
            "DistroResult retry should not be immediately due"
        );
    }

    #[tokio::test]
    async fn test_distro_retries_returned_and_removed() {
        let scheduler = DownloadSchedulerHandle::new(2, fast_config());

        let ticket1 = dummy_ticket(1);
        let ticket2 = dummy_ticket(2);
        let hash1 = ticket1.hash();
        let hash2 = ticket2.hash();
        scheduler
            .queue_failed_download(ticket1, Tag::from("distro-1"), distro_download_type())
            .await;
        scheduler
            .queue_failed_download(ticket2, Tag::from("distro-2"), distro_download_type())
            .await;

        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(scheduler.get_due_distro_retries().await.len(), 2);
        assert!(
            scheduler.get_due_distro_retries().await.is_empty(),
            "Retries should have been removed after first call"
        );
        assert!(!scheduler.remove_retry(hash1).await);
        assert!(!scheduler.remove_retry(hash2).await);
    }

    #[tokio::test]
    async fn test_distro_retries_dont_consume_capacity() {
        let scheduler = DownloadSchedulerHandle::new(1, fast_config());
        scheduler.wait_for_capacity().await.unwrap();

        scheduler
            .queue_failed_download(
                dummy_ticket(1),
                Tag::from("distro-1"),
                distro_download_type(),
            )
            .await;

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            scheduler.get_due_distro_retries().await.len(),
            1,
            "Distro retries should not be gated by capacity"
        );
    }

    #[tokio::test]
    async fn test_remove_retry() {
        let scheduler = DownloadSchedulerHandle::new(2, RetryConfig::default());

        let ticket = dummy_ticket(1);
        let hash = ticket.hash();
        scheduler
            .queue_failed_download(ticket, Tag::from("param-1"), param_download_type(1))
            .await;

        assert!(scheduler.remove_retry(hash).await);
        assert!(!scheduler.remove_retry(hash).await);
    }

    #[tokio::test]
    async fn test_distro_max_retries_exceeded() {
        let config = RetryConfig {
            backoff_base: Duration::from_millis(1),
            max_distro_retries: 2,
        };
        let scheduler = DownloadSchedulerHandle::new(2, config);

        let ticket = dummy_ticket(1);
        let tag = Tag::from("distro-1");
        let dt = distro_download_type();

        assert_eq!(
            scheduler
                .queue_failed_download(ticket.clone(), tag.clone(), dt.clone())
                .await,
            RetryQueueResult::Queued
        );
        assert_eq!(
            scheduler
                .queue_failed_download(ticket.clone(), tag.clone(), dt.clone())
                .await,
            RetryQueueResult::Queued
        );
        assert_eq!(
            scheduler
                .queue_failed_download(ticket.clone(), tag.clone(), dt.clone())
                .await,
            RetryQueueResult::MaxRetriesExceeded
        );
    }

    #[tokio::test]
    async fn test_model_sharing_never_exceeds_max_retries() {
        let config = RetryConfig {
            backoff_base: Duration::from_millis(1),
            max_distro_retries: 1,
        };
        let scheduler = DownloadSchedulerHandle::new(10, config);

        let ticket = dummy_ticket(1);
        let tag = Tag::from("param-1");
        let dt = param_download_type(1);

        for _ in 0..10 {
            assert_eq!(
                scheduler
                    .queue_failed_download(ticket.clone(), tag.clone(), dt.clone())
                    .await,
                RetryQueueResult::Queued
            );
            scheduler.start_parameter_retry().await;
        }
    }

    #[tokio::test]
    async fn test_wait_for_capacity_errors_on_actor_shutdown() {
        let (tx, rx) = mpsc::unbounded_channel::<SchedulerMessage>();
        drop(rx);

        let dead_scheduler = DownloadSchedulerHandle { tx };
        assert!(dead_scheduler.wait_for_capacity().await.is_err());
    }
}
