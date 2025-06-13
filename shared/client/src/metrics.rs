use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use opentelemetry::{
    global,
    metrics::{Counter, Gauge},
    KeyValue,
};
use psyche_network::{ConnectionType, DownloadType, Hash, NodeId};
use psyche_watcher::OpportunisticData;
use sysinfo::System;
use tokio::time::interval;

use crate::protocol::{Broadcast, BroadcastType};

#[derive(Clone, Debug)]
/// metrics collector for Psyche clients
pub struct ClientMetrics {
    // opentelemtery instruments
    pub(crate) broadcasts_seen_counter: Counter<u64>,
    pub(crate) broadcasts_current_round_gauge: Gauge<u64>,
    pub(crate) broadcasts_previous_round_gauge: Gauge<u64>,
    pub(crate) witnesses_sent: Counter<u64>,
    pub(crate) blobs_downloaded_current_round_gauge: Gauge<u64>,
    pub(crate) blobs_downloaded_previous_round_gauge: Gauge<u64>,
    pub(crate) peer_connections_gauge: Gauge<u64>,
    pub(crate) downloads_started_counter: Counter<u64>,
    pub(crate) downloads_finished_counter: Counter<u64>,
    pub(crate) downloads_failed_counter: Counter<u64>,
    pub(crate) downloads_bytes_gauge: Counter<u64>,
    pub(crate) gossip_peers_gauge: Gauge<u64>,
    pub(crate) cpu_usage_gauge: Gauge<f64>,
    pub(crate) memory_usage_gauge: Gauge<u64>,
    pub(crate) gpu_temperature_gauge: Gauge<f64>,
    pub(crate) round_step_gauge: Gauge<u64>,
    pub(crate) connection_latency_gauge: Gauge<f64>,
    pub(crate) bandwidth_gauge: Gauge<f64>,

    // internal state tracking
    pub(crate) last_seen_step: Arc<AtomicU32>,
    pub(crate) seen_broadcasts: Arc<Mutex<HashSet<[u8; 32]>>>,
    pub(crate) current_round_broadcasts: Arc<Mutex<HashSet<[u8; 32]>>>,
    pub(crate) previous_round_broadcasts: Arc<Mutex<HashSet<[u8; 32]>>>,
    pub(crate) current_round_downloads: Arc<Mutex<HashSet<[u8; 32]>>>,
    pub(crate) previous_round_downloads: Arc<Mutex<HashSet<[u8; 32]>>>,
    pub(crate) active_downloads: Arc<Mutex<HashMap<Hash, DownloadProgress>>>,
    pub(crate) system_monitor: Arc<Mutex<System>>,
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    total_size: u64,
    downloaded_size: u64,
    download_type: DownloadType,
    source_peer: NodeId,
}

impl ClientMetrics {
    pub fn new() -> Self {
        let meter = global::meter("psyche_client");

        Self {
            // broadcasts
            broadcasts_seen_counter: meter
                .u64_counter("psyche_broadcasts_seen_total")
                .with_description("Total number of broadcasts seen by this node")
                .build(),
            broadcasts_current_round_gauge: meter
                .u64_gauge("psyche_broadcasts_current_round")
                .with_description("Number of broadcasts seen in current round")
                .build(),
            broadcasts_previous_round_gauge: meter
                .u64_gauge("psyche_broadcasts_previous_round")
                .with_description("Number of broadcasts seen in previous round")
                .build(),
            blobs_downloaded_current_round_gauge: meter
                .u64_gauge("psyche_blobs_downloaded_current_round")
                .with_description("Number of blob downloaded in current round")
                .build(),
            blobs_downloaded_previous_round_gauge: meter
                .u64_gauge("psyche_blobs_downloaded_previous_round")
                .with_description("Number of blob downloaded in previous round")
                .build(),

            // witness
            witnesses_sent: meter
                .u64_counter("psyche_witnesses_sent_total")
                .with_description("Total number of witness transactions sent")
                .build(),

            // network
            peer_connections_gauge: meter
                .u64_gauge("psyche_peer_connections")
                .with_description("Number of peer connections by type")
                .build(),
            gossip_peers_gauge: meter
                .u64_gauge("psyche_gossip_peers")
                .with_description("Number of peers in gossip network")
                .build(),
            bandwidth_gauge: meter
                .f64_gauge("psyche_bandwidth_bytes_per_second")
                .with_description("Current bandwidth usage in bytes per second")
                .build(),

            // download metrics
            downloads_started_counter: meter
                .u64_counter("psyche_downloads_started_total")
                .with_description("Total number of downloads started")
                .build(),
            downloads_finished_counter: meter
                .u64_counter("psyche_downloads_finished_total")
                .with_description("Total number of downloads completed successfully")
                .build(),
            downloads_failed_counter: meter
                .u64_counter("psyche_downloads_failed_total")
                .with_description("Total number of downloads that failed")
                .build(),
            downloads_bytes_gauge: meter
                .u64_counter("psyche_download_bytes")
                .with_description("Total number of bytes recv'd thru blobs")
                .build(),

            // system metrics
            cpu_usage_gauge: meter
                .f64_gauge("psyche_cpu_usage_percent")
                .with_description("CPU usage percentage")
                .build(),
            memory_usage_gauge: meter
                .u64_gauge("psyche_memory_usage_bytes")
                .with_description("Memory usage in bytes")
                .build(),
            gpu_temperature_gauge: meter
                .f64_gauge("psyche_gpu_temperature_celsius")
                .with_description("GPU temperature in Celsius")
                .build(),

            // state metrics
            round_step_gauge: meter
                .u64_gauge("psyche_round_step")
                .with_description("Current step in the training round")
                .build(),
            connection_latency_gauge: meter
                .f64_gauge("psyche_connection_latency_seconds")
                .with_description("Connection latency to peers")
                .build(),

            // internal state
            seen_broadcasts: Arc::new(Mutex::new(HashSet::new())),
            current_round_broadcasts: Arc::new(Mutex::new(HashSet::new())),
            previous_round_broadcasts: Arc::new(Mutex::new(HashSet::new())),
            current_round_downloads: Arc::new(Mutex::new(HashSet::new())),
            previous_round_downloads: Arc::new(Mutex::new(HashSet::new())),
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            system_monitor: Arc::new(Mutex::new(System::new_all())),
            last_seen_step: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn record_broadcast_seen(&self, broadcast: &Broadcast, current_step: u32) {
        let broadcast_hash =
            psyche_core::sha256(&postcard::to_allocvec(broadcast).unwrap_or_default());

        let mut seen = self.seen_broadcasts.lock().unwrap();
        if seen.insert(broadcast_hash) {
            self.broadcasts_seen_counter.add(
                1,
                &[
                    KeyValue::new(
                        "broadcast_type",
                        match &broadcast.data {
                            BroadcastType::TrainingResult(_) => "training_result",
                            BroadcastType::Finished(_) => "finished",
                        },
                    ),
                    KeyValue::new(
                        "broadcast_data",
                        match &broadcast.data {
                            BroadcastType::TrainingResult(tr) => tr.batch_id.to_string(),
                            BroadcastType::Finished(f) => if f.warmup {
                                "warmup_finished"
                            } else {
                                "finished"
                            }
                            .to_string(),
                        },
                    ),
                    KeyValue::new("step", broadcast.step.to_string()),
                ],
            );

            // track by round
            if broadcast.step == current_step {
                self.current_round_broadcasts
                    .lock()
                    .unwrap()
                    .insert(broadcast_hash);
            } else if broadcast.step == current_step.saturating_sub(1) {
                self.previous_round_broadcasts
                    .lock()
                    .unwrap()
                    .insert(broadcast_hash);
            }

            self.update_round_state(current_step);
        }
    }

    pub fn record_witness_send(&self, data: &OpportunisticData) {
        self.witnesses_sent.add(
            1,
            &[KeyValue::new(
                "type",
                match data {
                    OpportunisticData::WarmupStep(_) => "warmup",
                    OpportunisticData::WitnessStep(..) => "witness",
                },
            )],
        );
    }

    pub fn update_peer_connections(
        &self,
        connections: &[(psyche_network::NodeAddr, ConnectionType, f32)],
    ) {
        let mut connection_counts = HashMap::new();

        for (addr, conn_type, ping) in connections {
            let conn_type_str = match conn_type {
                ConnectionType::Direct(_) => "direct",
                ConnectionType::Relay(_) => "relay",
                ConnectionType::Mixed(_, _) => "mixed",
                ConnectionType::None => "none",
            };
            *connection_counts.entry(conn_type_str).or_insert(0u64) += 1;

            self.connection_latency_gauge.record(
                (*ping).into(),
                &[KeyValue::new("ping", addr.node_id.to_string())],
            );
        }

        // record connection counts by type
        for (conn_type, count) in connection_counts {
            self.peer_connections_gauge
                .record(count, &[KeyValue::new("connection_type", conn_type)]);
        }
    }

    pub fn update_download_progress(
        &self,
        hash: Hash,
        source_peer: NodeId,
        download_type: DownloadType,
        downloaded_size: u64,
        total_size: u64,
    ) {
        if let Ok(mut downloads) = self.active_downloads.lock() {
            let progress = downloads.entry(hash).or_insert_with(|| {
                let progress = DownloadProgress {
                    total_size: 0,
                    downloaded_size: 0,
                    download_type: download_type.clone(),
                    source_peer,
                };

                let download_type_str = match download_type {
                    DownloadType::DistroResult(_) => "distro_result",
                    DownloadType::ModelSharing(_) => "model_sharing",
                };

                self.downloads_started_counter.add(
                    1,
                    &[
                        KeyValue::new("download_type", download_type_str),
                        KeyValue::new("source_peer", source_peer.to_string()),
                    ],
                );
                progress
            });

            let delta = downloaded_size - progress.downloaded_size;
            progress.downloaded_size = downloaded_size;
            progress.total_size = total_size;

            self.downloads_bytes_gauge
                .add(delta, &[KeyValue::new("hash", hash.to_string())]);
        }
    }

    pub fn record_download_completed(&self, hash: Hash) {
        if let Ok(mut downloads) = self.active_downloads.lock() {
            if let Some(progress) = downloads.remove(&hash) {
                let download_type_str = match progress.download_type {
                    DownloadType::DistroResult(_) => "distro_result",
                    DownloadType::ModelSharing(_) => "model_sharing",
                };

                self.downloads_finished_counter.add(
                    1,
                    &[
                        KeyValue::new("download_type", download_type_str),
                        KeyValue::new("source_peer", progress.source_peer.to_string()),
                    ],
                );
            }
        }
    }

    pub fn record_download_failed(&self, hash: Hash, error: &str) {
        if let Ok(mut downloads) = self.active_downloads.lock() {
            if let Some(progress) = downloads.remove(&hash) {
                let download_type_str = match progress.download_type {
                    DownloadType::DistroResult(_) => "distro_result",
                    DownloadType::ModelSharing(_) => "model_sharing",
                };

                self.downloads_failed_counter.add(
                    1,
                    &[
                        KeyValue::new("download_type", download_type_str),
                        KeyValue::new("source_peer", progress.source_peer.to_string()),
                        KeyValue::new("error", error.to_string()),
                    ],
                );
            }
        }
    }

    pub fn update_gossip_peers(&self, peer_count: usize) {
        self.gossip_peers_gauge.record(peer_count as u64, &[]);
    }

    pub fn update_bandwidth(&self, bytes_per_second: f64) {
        self.bandwidth_gauge.record(bytes_per_second, &[]);
    }

    pub fn update_round_state(&self, step: u32) {
        self.round_step_gauge.record(step as u64, &[]);

        // update broadcast counts for current round
        {
            let current_count = self.current_round_broadcasts.lock().unwrap().len() as u64;
            let previous_count = self.previous_round_broadcasts.lock().unwrap().len() as u64;

            self.broadcasts_current_round_gauge
                .record(current_count, &[]);

            self.broadcasts_previous_round_gauge
                .record(previous_count, &[]);
        }

        // update download counts for current round
        {
            let current_count = self.current_round_downloads.lock().unwrap().len() as u64;
            let previous_count = self.previous_round_downloads.lock().unwrap().len() as u64;

            self.blobs_downloaded_current_round_gauge
                .record(current_count, &[]);

            self.blobs_downloaded_previous_round_gauge
                .record(previous_count, &[]);
        }

        if step != self.last_seen_step.load(Ordering::Relaxed) {
            {
                let mut current = self.current_round_broadcasts.lock().unwrap();
                let mut previous = self.previous_round_broadcasts.lock().unwrap();

                *previous = std::mem::take(&mut current);
            }
            {
                let mut current = self.current_round_downloads.lock().unwrap();
                let mut previous = self.previous_round_downloads.lock().unwrap();

                *previous = std::mem::take(&mut current);
            }

            self.last_seen_step.store(step, Ordering::Relaxed);
        }
    }

    pub fn start_system_monitoring(&self) -> tokio::task::JoinHandle<()> {
        let cpu_gauge = self.cpu_usage_gauge.clone();
        let memory_gauge = self.memory_usage_gauge.clone();
        let gpu_temp_gauge = self.gpu_temperature_gauge.clone();
        let system_monitor = self.system_monitor.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));

            loop {
                if let Ok(mut system) = system_monitor.lock() {
                    system.refresh_all();

                    let cpu_usage = system.global_cpu_usage() as f64;
                    cpu_gauge.record(cpu_usage, &[]);

                    let memory_used = system.used_memory();
                    memory_gauge.record(memory_used, &[]);

                    if let Some(temp) = Self::get_gpu_temperature() {
                        gpu_temp_gauge.record(temp, &[]);
                    }
                }
                interval.tick().await;
            }
        })
    }

    // placeholder for GPU temperature
    fn get_gpu_temperature() -> Option<f64> {
        // TODO: pull from CUDA / nvidia crate?
        None
    }
}
#[cfg(test)]
mod tests {
    use crate::{Finished, TrainingResult};

    use super::*;
    use psyche_coordinator::{Commitment, CommitteeProof};
    use psyche_core::{BatchId, ClosedInterval, MerkleRoot};
    use psyche_network::{BlobFormat, BlobTicket, ModelRequestType, NodeAddr, PublicKey};
    use std::sync::Arc;
    use tokio::time::{sleep, Duration};

    const DUMMY_KEY: [u8; 32] = [
        0x06, 0x18, 0xc7, 0x13, 0xa0, 0x0e, 0x6b, 0x7d, 0xea, 0xd8, 0x91, 0xa5, 0x56, 0x4f, 0x4e,
        0x8c, 0x80, 0x91, 0xa7, 0x64, 0xba, 0x77, 0xd9, 0xd4, 0x26, 0x1d, 0x17, 0xaf, 0xa3, 0xd4,
        0x7a, 0x0a,
    ];

    fn create_test_broadcast(step: u32, broadcast_type: BroadcastType) -> Broadcast {
        Broadcast {
            commitment: Commitment {
                data_hash: [0; 32],
                signature: [0; 64],
            },
            step,
            data: broadcast_type,
            nonce: 0,
            proof: CommitteeProof::default(),
        }
    }

    fn create_test_blob_ticket() -> BlobTicket {
        BlobTicket::new(
            NodeAddr::new(PublicKey::from_bytes(&DUMMY_KEY).unwrap()),
            Hash::from_bytes([0; 32]),
            BlobFormat::Raw,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn test_broadcast_metrics() {
        let metrics = ClientMetrics::new();
        let step = 5;

        // Record different types of broadcasts
        let training_broadcast = create_test_broadcast(
            5,
            BroadcastType::TrainingResult(TrainingResult {
                batch_id: BatchId(ClosedInterval::new(0, 0)),
                ticket: create_test_blob_ticket(),
            }),
        );

        let finished_broadcast = create_test_broadcast(
            4,
            BroadcastType::Finished(Finished {
                warmup: true,
                broadcast_merkle: MerkleRoot::new([0; 32]),
            }),
        );

        // First time seeing broadcasts
        metrics.record_broadcast_seen(&training_broadcast, step);
        metrics.record_broadcast_seen(&finished_broadcast, step);

        // Duplicate broadcast should not increment counter
        metrics.record_broadcast_seen(&training_broadcast, step);

        // Verify internal state
        {
            let seen = metrics.seen_broadcasts.lock().unwrap();
            assert_eq!(seen.len(), 2);

            let current = metrics.current_round_broadcasts.lock().unwrap();
            assert_eq!(current.len(), 1); // Only training_broadcast is in current round

            let previous = metrics.previous_round_broadcasts.lock().unwrap();
            assert_eq!(previous.len(), 1); // finished_broadcast is in previous round
        }
    }

    #[tokio::test]
    async fn test_peer_connection_metrics() {
        let metrics = ClientMetrics::new();

        let connections = vec![
            (
                NodeAddr::new(PublicKey::from_bytes(&DUMMY_KEY).unwrap()),
                ConnectionType::Direct("10.0.0.1:1234".parse().unwrap()),
                0.025,
            ),
            (
                NodeAddr::new(PublicKey::from_bytes(&DUMMY_KEY).unwrap()),
                ConnectionType::Relay("https://bingus.com".parse().unwrap()),
                0.150,
            ),
            (
                NodeAddr::new(PublicKey::from_bytes(&DUMMY_KEY).unwrap()),
                ConnectionType::Mixed(
                    "10.0.0.3:1234".parse().unwrap(),
                    "https://bingus.gg".parse().unwrap(),
                ),
                0.075,
            ),
        ];

        metrics.update_peer_connections(&connections);
    }

    #[tokio::test]
    async fn test_download_lifecycle() {
        let metrics = ClientMetrics::new();

        let hash = [42u8; 32].into();
        let peer_id = PublicKey::from_bytes(&DUMMY_KEY).unwrap();
        let download_type = DownloadType::DistroResult(Default::default());

        // Verify download is tracked
        {
            let downloads = metrics.active_downloads.lock().unwrap();
            assert!(downloads.contains_key(&hash));
        }

        // Update progress multiple times
        metrics.update_download_progress(hash, peer_id, download_type.clone(), 1024, 10240);
        metrics.update_download_progress(hash, peer_id, download_type.clone(), 5120, 10240);
        metrics.update_download_progress(hash, peer_id, download_type, 10240, 10240);

        // Complete download
        metrics.record_download_completed(hash);

        // Verify download is removed
        {
            let downloads = metrics.active_downloads.lock().unwrap();
            assert!(!downloads.contains_key(&hash));
        }
    }

    #[tokio::test]
    async fn test_download_failure() {
        let metrics = ClientMetrics::new();

        let hash = [43u8; 32].into();
        let peer_id = PublicKey::from_bytes(&DUMMY_KEY).unwrap();
        let download_type = DownloadType::ModelSharing(ModelRequestType::Config);

        // Start and fail download
        metrics.update_download_progress(hash, peer_id, download_type, 2048, 8192);
        metrics.record_download_failed(hash, "connection timeout");

        // Verify download is removed after failure
        {
            let downloads = metrics.active_downloads.lock().unwrap();
            assert!(!downloads.contains_key(&hash));
        }
    }

    #[tokio::test]
    async fn test_gossip_and_bandwidth_metrics() {
        let metrics = ClientMetrics::new();

        metrics.update_gossip_peers(15);
        metrics.update_bandwidth(1024.0 * 1024.0); // 1 MB/s
    }

    #[tokio::test]
    async fn test_round_rotation() {
        let metrics = ClientMetrics::new();
        let step = 1;

        for i in 0..3 {
            let broadcast = create_test_broadcast(
                1,
                BroadcastType::TrainingResult(TrainingResult {
                    batch_id: BatchId(ClosedInterval::new(i, i)),
                    ticket: create_test_blob_ticket(),
                }),
            );
            metrics.record_broadcast_seen(&broadcast, step);
        }

        {
            let current = metrics.current_round_broadcasts.lock().unwrap();
            assert_eq!(
                current.len(),
                3,
                "current round has wrong amount of broadcasts"
            );
            let previous = metrics.previous_round_broadcasts.lock().unwrap();
            assert_eq!(previous.len(), 0, "previous round should have 0 broadcasts");
        }

        let step = 2;
        metrics.update_round_state(step);

        {
            let current = metrics.current_round_broadcasts.lock().unwrap();
            let previous = metrics.previous_round_broadcasts.lock().unwrap();
            assert_eq!(current.len(), 0);
            assert_eq!(previous.len(), 3);
        }

        let new_broadcast = create_test_broadcast(
            step,
            BroadcastType::Finished(Finished {
                warmup: false,
                broadcast_merkle: MerkleRoot::new([0; 32]),
            }),
        );
        metrics.record_broadcast_seen(&new_broadcast, step);
        let step = 3;
        metrics.update_round_state(step);

        {
            let current = metrics.current_round_broadcasts.lock().unwrap();
            assert_eq!(current.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_system_monitoring() {
        let metrics = Arc::new(ClientMetrics::new());

        // Start monitoring
        let handle = metrics.start_system_monitoring();

        // Let it run for a bit
        sleep(Duration::from_millis(100)).await;

        // Cancel the monitoring task
        handle.abort();

        // Verify system was accessed (by checking it's initialized)
        let system = metrics.system_monitor.lock().unwrap();
        assert!(system.total_memory() > 0);
    }

    #[tokio::test]
    async fn test_edge_cases() {
        let metrics = ClientMetrics::new();

        // Test with empty/zero values
        metrics.update_gossip_peers(0);
        metrics.update_bandwidth(0.0);

        // Test with very large values
        let huge_hash = [255u8; 32].into();
        metrics.update_download_progress(
            huge_hash,
            PublicKey::from_bytes(&DUMMY_KEY).unwrap(),
            DownloadType::ModelSharing(psyche_network::ModelRequestType::Config),
            u64::MAX / 2,
            u64::MAX,
        );

        // Test round state with edge values
        metrics.update_round_state(u32::MAX);

        // Test finishing non-existent download
        let non_existent_hash = [123u8; 32].into();
        metrics.record_download_completed(non_existent_hash);
        metrics.record_download_failed(non_existent_hash, "doesn't exist");
    }
}
