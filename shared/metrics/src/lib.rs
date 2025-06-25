mod iroh;

use std::{
    collections::HashMap,
    fmt::Display,
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use nvml_wrapper::{enum_wrappers::device::TemperatureSensor, Nvml};
use opentelemetry::{
    global,
    metrics::{Counter, Gauge, Histogram, Meter},
    KeyValue,
};
use sysinfo::System;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    time::interval,
};

pub use iroh::{create_iroh_registry, IrohMetricsCollector};
pub use iroh_metrics::Registry as IrohMetricsRegistry;
use tracing::debug;

#[derive(Clone, Debug)]
/// metrics collector for Psyche clients
pub struct ClientMetrics {
    // opentelemtery instruments

    // broadcasts and applying messages
    pub(crate) broadcasts_seen_counter: Counter<u64>,
    pub(crate) apply_message_success_counter: Counter<u64>,
    pub(crate) apply_message_failure_counter: Counter<u64>,
    pub(crate) apply_message_ignored_counter: Counter<u64>,

    pub(crate) witnesses_sent: Counter<u64>,

    pub(crate) peer_connections: Gauge<u64>,
    pub(crate) gossip_neighbors: Gauge<u64>,

    pub(crate) downloads_started_counter: Counter<u64>,
    pub(crate) downloads_finished_counter: Counter<u64>,
    pub(crate) downloads_retry_counter: Counter<u64>,
    pub(crate) downloads_failed_counter: Counter<u64>,
    pub(crate) downloads_perma_failed_counter: Counter<u64>,
    pub(crate) downloads_bytes_counter: Counter<u64>,

    pub(crate) round_step_gauge: Gauge<u64>,
    pub(crate) connection_latency: Histogram<f64>,
    pub(crate) bandwidth: Gauge<f64>,

    /// Just a boolean
    pub(crate) participating_in_round: Gauge<u64>,

    // internal state tracking
    pub(crate) system_monitor: Arc<tokio::task::JoinHandle<()>>,
    pub(crate) tcp_server: Arc<tokio::task::JoinHandle<()>>,

    // shared state for TCP server
    pub(crate) shared_state: Arc<MetricsState>,
}

#[derive(Debug)]
struct MetricsState {
    peer_connection_count: AtomicU64,
    current_bandwidth: Arc<Mutex<f64>>,
    current_round_step: AtomicU32,
    participating: AtomicU64,
}

impl Drop for ClientMetrics {
    fn drop(&mut self) {
        self.system_monitor.abort();
        self.tcp_server.abort();
    }
}

pub enum ConnectionType {
    Direct,
    Mixed,
    Relay,
}

pub struct PeerConnection {
    pub node_id: String,
    pub connection_type: ConnectionType,
    pub latency: f32,
}

pub enum ClientRoleInRound {
    NotInRound,
    Trainer,
    Witness,
}

impl ClientMetrics {
    pub fn new() -> Self {
        let meter = global::meter("psyche_client");

        let shared_state = Arc::new(MetricsState {
            peer_connection_count: AtomicU64::new(0),
            current_bandwidth: Arc::new(Mutex::new(0.0)),
            current_round_step: AtomicU32::new(0),
            participating: AtomicU64::new(0),
        });

        let tcp_server = Self::start_tcp_server(shared_state.clone());

        Self {
            // broadcasts state
            broadcasts_seen_counter: meter
                .u64_counter("psyche_broadcasts_seen_total")
                .with_description("Total number of broadcasts seen by this node")
                .build(),
            apply_message_success_counter: meter
                .u64_counter("psyche_apply_message_success")
                .with_description("Number of successfully applied broadcasts")
                .build(),
            apply_message_failure_counter: meter
                .u64_counter("psyche_apply_message_failure")
                .with_description("Number of broadcasts we failed to apply")
                .build(),
            apply_message_ignored_counter: meter
                .u64_counter("psyche_apply_message_ignored")
                .with_description(
                    "Number of broadcasts we ignored during apply, probably due to rebroadcast",
                )
                .build(),

            // downloads
            downloads_started_counter: meter
                .u64_counter("psyche_downloads_started")
                .with_description("Number of downloads started")
                .build(),
            downloads_finished_counter: meter
                .u64_counter("psyche_downloads_finished")
                .with_description("Number of downloads finished")
                .build(),
            downloads_retry_counter: meter
                .u64_counter("psyche_downloads_retry")
                .with_description("Number of downloads retried")
                .build(),
            downloads_failed_counter: meter
                .u64_counter("psyche_downloads_failed_total")
                .with_description("Total number of download attempts that failed")
                .build(),
            downloads_perma_failed_counter: meter
                .u64_counter("psyche_downloads_failed_total")
                .with_description("Total number of downloads that permantently failed")
                .build(),
            downloads_bytes_counter: meter
                .u64_counter("psyche_download_bytes")
                .with_description("Total number of bytes recv'd thru blobs")
                .build(),

            // witness
            witnesses_sent: meter
                .u64_counter("psyche_witnesses_sent_total")
                .with_description("Total number of witness transactions sent")
                .build(),
            participating_in_round: meter
                .u64_gauge("psyche_participating_in_round")
                .with_description("Whether or not this node is participating in this round")
                .build(),
            round_step_gauge: meter
                .u64_gauge("psyche_round_step")
                .with_description("Current step in the training round")
                .build(),

            // network
            peer_connections: meter
                .u64_gauge("psyche_peer_connections")
                .with_description("Number of peer connections by type")
                .build(),
            gossip_neighbors: meter
                .u64_gauge("psyche_gossip_neighbors")
                .with_description("Number of neighbors in gossip network")
                .build(),
            bandwidth: meter
                .f64_gauge("psyche_bandwidth_bytes_per_second")
                .with_description("Current bandwidth usage in bytes per second")
                .build(),
            connection_latency: meter
                .f64_histogram("psyche_connection_latency_seconds")
                .with_description("Connection latency to peers")
                .build(),

            system_monitor: Self::start_system_monitoring(&meter),
            tcp_server,
            shared_state,
        }
    }

    pub fn record_broadcast_seen(&self) {
        self.broadcasts_seen_counter.add(1, &[]);
    }

    pub fn record_apply_message_success(&self, step: u32, from_peer: impl Display, kind: &str) {
        debug!(name: "apply_message_success", step=%step, kind=%kind, from=%from_peer);
        self.apply_message_success_counter.add(
            1,
            &[
                KeyValue::new("step", step as i64),
                KeyValue::new("type", kind.to_string()),
            ],
        );
    }

    pub fn record_apply_message_failure(&self, step: u32, from_peer: impl Display, kind: &str) {
        debug!(name: "apply_message_failure", step=%step, kind=%kind, from=%from_peer);
        self.apply_message_failure_counter.add(
            1,
            &[
                KeyValue::new("step", step as i64),
                KeyValue::new("type", kind.to_string()),
            ],
        )
    }

    pub fn record_apply_message_ignored(&self, step: u32, kind: impl Display) {
        self.apply_message_ignored_counter.add(
            1,
            &[
                KeyValue::new("step", step as i64),
                KeyValue::new("type", kind.to_string()),
            ],
        )
    }

    pub fn record_witness_send(&self, kind: impl Display) {
        self.witnesses_sent
            .add(1, &[KeyValue::new("type", kind.to_string())]);
    }

    pub fn record_download_started(&self, hash: impl Display, kind: impl Display) {
        debug!(name: "download_started", hash = %hash);
        self.downloads_started_counter
            .add(1, &[KeyValue::new("type", kind.to_string())]);
    }
    pub fn record_download_retry(&self, hash: impl Display) {
        debug!(name: "download_retry", hash = %hash);
        self.downloads_retry_counter.add(1, &[]);
    }

    pub fn update_download_progress(&self, hash: impl Display, newly_downloaded_bytes: u64) {
        self.downloads_bytes_counter.add(
            newly_downloaded_bytes,
            &[KeyValue::new("hash", hash.to_string())],
        );
    }

    pub fn record_download_completed(&self, hash: impl Display, from_peer: impl Display) {
        debug!(
            name:"download_complete",
            hash =%hash,
            from_peer =%from_peer
        );
        self.downloads_finished_counter.add(1, &[]);
    }

    pub fn record_download_failed(&self) {
        self.downloads_failed_counter.add(1, &[]);
    }
    pub fn record_download_perma_failed(&self) {
        self.downloads_perma_failed_counter.add(1, &[]);
    }

    pub fn update_peer_connections(&self, connections: &[PeerConnection]) {
        let mut connection_counts = HashMap::new();

        for PeerConnection {
            node_id,
            connection_type,
            latency,
        } in connections
        {
            *connection_counts
                .entry(match connection_type {
                    ConnectionType::Direct => "direct",
                    ConnectionType::Mixed => "mixed",
                    ConnectionType::Relay => "relay",
                })
                .or_insert(0u64) += 1;

            self.connection_latency
                .record((*latency).into(), &[KeyValue::new("ping", node_id.clone())]);
        }

        // Update shared state
        self.shared_state
            .peer_connection_count
            .store(connections.len() as u64, Ordering::Relaxed);

        // record connection counts by type
        for (conn_type, count) in connection_counts {
            self.peer_connections
                .record(count, &[KeyValue::new("connection_type", conn_type)]);
        }
    }

    pub fn update_p2p_gossip_neighbors(&self, neighbors: &[impl Display]) {
        self.gossip_neighbors.record(
            neighbors.len() as u64,
            &[KeyValue::new(
                "peers",
                neighbors
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            )],
        );
    }

    pub fn update_bandwidth(&self, bytes_per_second: f64) {
        self.bandwidth.record(bytes_per_second, &[]);
        *self.shared_state.current_bandwidth.lock().unwrap() = bytes_per_second;
    }

    pub fn update_round_state(&self, step: u32, role: ClientRoleInRound) {
        self.round_step_gauge.record(step as u64, &[]);
        self.shared_state
            .current_round_step
            .store(step, Ordering::Relaxed);

        let participating = !matches!(role, ClientRoleInRound::NotInRound) as u64;
        self.participating_in_round.record(
            participating,
            &[KeyValue::new(
                "role",
                match role {
                    ClientRoleInRound::NotInRound => "not_in_round",
                    ClientRoleInRound::Trainer => "trainer",
                    ClientRoleInRound::Witness => "witness",
                },
            )],
        );
        self.shared_state
            .participating
            .store(participating, Ordering::Relaxed);
    }

    fn start_system_monitoring(meter: &Meter) -> Arc<tokio::task::JoinHandle<()>> {
        let mut interval = interval(Duration::from_secs(5));
        let system = Arc::new(Mutex::new(System::new_all()));

        let cpu_usage = meter
            .f64_gauge("psyche_cpu_usage_percent")
            .with_description("CPU usage percentage")
            .build();
        let memory_usage = meter
            .u64_gauge("psyche_memory_usage_bytes")
            .with_description("Memory usage in bytes")
            .build();

        struct GpuMeters {
            nvml: Nvml,
            gpu_usage: Histogram<f64>,
            gpu_memory: Histogram<u64>,
            gpu_temp: Histogram<u64>,
        }

        let gpu_meters = Nvml::init().ok().map(|nvml| GpuMeters {
            nvml,
            gpu_usage: meter
                .f64_histogram("psyche_gpu_usage_percent")
                .with_description("GPU usage percentage")
                .build(),
            gpu_memory: meter
                .u64_histogram("psyche_gpu_memory")
                .with_description("GPU memory usage")
                .build(),
            gpu_temp: meter
                .u64_histogram("psyche_gpu_temp")
                .with_description("GPU usage percentage")
                .build(),
        });
        Arc::new(tokio::spawn(async move {
            loop {
                let system_clone = system.clone();
                tokio::task::spawn_blocking(move || system_clone.lock().unwrap().refresh_all())
                    .await
                    .unwrap();

                cpu_usage.record(system.lock().unwrap().global_cpu_usage() as f64, &[]);

                memory_usage.record(system.lock().unwrap().used_memory(), &[]);

                if let Some(GpuMeters {
                    gpu_usage,
                    gpu_memory,
                    gpu_temp,
                    nvml,
                }) = &gpu_meters
                {
                    for (i, gpu) in (0..nvml.device_count().unwrap())
                        .map(|i| (i, nvml.device_by_index(i).unwrap()))
                    {
                        let device_info = [KeyValue::new("gpu", i as i64)];
                        gpu_usage.record(gpu.utilization_rates().unwrap().gpu as f64, &device_info);
                        let memory_info = gpu.memory_info().unwrap();
                        gpu_memory.record(memory_info.used, &device_info);
                        gpu_temp.record(
                            gpu.temperature(TemperatureSensor::Gpu).unwrap() as u64,
                            &device_info,
                        );
                    }
                }
                interval.tick().await;
            }
        }))
    }

    fn start_tcp_server(shared_state: Arc<MetricsState>) -> Arc<tokio::task::JoinHandle<()>> {
        Arc::new(tokio::spawn(async move {
            let listener = match TcpListener::bind("127.0.0.1:8889").await {
                Ok(listener) => listener,
                Err(e) => {
                    eprintln!("Failed to bind TCP server: {}", e);
                    return;
                }
            };

            let mut interval = interval(Duration::from_secs(5));
            let mut clients: Vec<TcpStream> = Vec::new();

            loop {
                tokio::select! {
                    // Accept new connections
                    Ok((stream, _)) = listener.accept() => {
                        clients.push(stream);
                    }

                    // Broadcast metrics every 5 seconds
                    _ = interval.tick() => {
                        let bandwidth = *shared_state.current_bandwidth.lock().unwrap();
                        let stats = format!(
                            "{{\"timestamp\":{},\"peer_connections\":{},\"bandwidth\":{:.2},\"round_step\":{},\"participating\":{}}}\n",
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                            shared_state.peer_connection_count.load(Ordering::Relaxed),
                            bandwidth,
                            shared_state.current_round_step.load(Ordering::Relaxed),
                            shared_state.participating.load(Ordering::Relaxed)
                        );

                        // Send to all connected clients, remove disconnected ones
                        let mut i = 0;
                        while i < clients.len() {
                            if clients[i].write_all(stats.as_bytes()).await.is_err() {
                                clients.remove(i);
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
            }
        }))
    }
}

impl Default for ClientMetrics {
    fn default() -> Self {
        Self::new()
    }
}
