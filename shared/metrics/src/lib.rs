mod iroh;

use std::{
    collections::HashMap,
    fmt::Display,
    sync::{Arc, Mutex},
    time::Duration,
};

use nvml_wrapper::{enum_wrappers::device::TemperatureSensor, Nvml};
use opentelemetry::{
    global,
    metrics::{Counter, Gauge, Histogram, Meter},
    KeyValue,
};
use serde::Serialize;
use sysinfo::System;
use tokio::{io::AsyncWriteExt, net::TcpListener, time::interval};

pub use iroh::{create_iroh_registry, IrohMetricsCollector};
pub use iroh_metrics::Registry as IrohMetricsRegistry;
use tracing::{debug, info, warn};

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
    pub(crate) tcp_server: Option<Arc<tokio::task::JoinHandle<()>>>,

    // shared state for TCP server
    pub(crate) tcp_metrics: Arc<Mutex<TcpMetrics>>,

    pub(crate) num_params: Option<u64>,

    // p2p model sharing
    pub(crate) p2p_downloaded_params_total: u64,
    pub(crate) p2p_downloaded_params_percent: Option<Gauge<f64>>,
    pub(crate) p2p_params_download_failed_counter: Counter<u64>,
}

#[derive(Serialize, Debug, Clone, Default)]
struct TcpMetrics {
    connected_peers: Vec<PeerConnection>,
    bandwidth: f64,
    round_step: u32,
    role: ClientRoleInRound,
    broadcasts_seen: u64,
    apply_message_success: u64,
    apply_message_failure: u64,
    apply_message_ignored: u64,
    witnesses_sent: u64,
    gossip_neighbors: Vec<String>,
    downloads_started: u64,
    downloads_finished: u64,
    downloads_retry: u64,
    downloads_failed: u64,
    downloads_perma_failed: u64,
    downloads_bytes: u64,
}

impl Drop for ClientMetrics {
    fn drop(&mut self) {
        self.system_monitor.abort();
        if let Some(server) = &self.tcp_server {
            server.abort();
        }
    }
}

#[derive(Debug, Serialize, Clone, Copy)]
pub enum ConnectionType {
    Direct,
    Mixed,
    Relay,
}

#[derive(Debug, Serialize, Clone)]
pub struct PeerConnection {
    pub node_id: String,
    pub connection_type: ConnectionType,
    pub latency: f32,
}

#[derive(Debug, Serialize, Clone, Copy)]
pub enum ClientRoleInRound {
    NotInRound,
    Trainer,
    Witness,
}

impl Default for ClientRoleInRound {
    fn default() -> Self {
        Self::NotInRound
    }
}

impl ClientMetrics {
    pub fn new(metrics_port: Option<u16>) -> Self {
        let meter = global::meter("psyche_client");

        let tcp_metrics = Arc::new(Mutex::new(TcpMetrics::default()));
        let tcp_server = metrics_port.map(|port| Self::start_tcp_server(port, tcp_metrics.clone()));

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
                .u64_counter("psyche_downloads_perma_failed_total")
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
            tcp_metrics,

            num_params: None,

            p2p_downloaded_params_total: 0,

            p2p_downloaded_params_percent: None,

            p2p_params_download_failed_counter: meter
                .u64_counter("psyche_p2p_params_download_failed_counter")
                .with_description("The total amount of p2p parameter sharing downloads that failed")
                .build(),
        }
    }

    pub fn record_broadcast_seen(&self) {
        self.broadcasts_seen_counter.add(1, &[]);
        self.tcp_metrics.lock().unwrap().broadcasts_seen += 1;
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
        self.tcp_metrics.lock().unwrap().apply_message_success += 1;
    }

    pub fn record_apply_message_failure(&self, step: u32, from_peer: impl Display, kind: &str) {
        debug!(name: "apply_message_failure", step=%step, kind=%kind, from=%from_peer);
        self.apply_message_failure_counter.add(
            1,
            &[
                KeyValue::new("step", step as i64),
                KeyValue::new("type", kind.to_string()),
            ],
        );
        self.tcp_metrics.lock().unwrap().apply_message_failure += 1;
    }

    pub fn record_apply_message_ignored(&self, step: u32, kind: impl Display) {
        self.apply_message_ignored_counter.add(
            1,
            &[
                KeyValue::new("step", step as i64),
                KeyValue::new("type", kind.to_string()),
            ],
        );
        self.tcp_metrics.lock().unwrap().apply_message_ignored += 1;
    }

    pub fn record_witness_send(&self, kind: impl Display) {
        self.witnesses_sent
            .add(1, &[KeyValue::new("type", kind.to_string())]);
        self.tcp_metrics.lock().unwrap().witnesses_sent += 1;
    }

    pub fn record_download_started(&self, hash: impl Display, kind: impl Display) {
        debug!(name: "download_started", hash = %hash);
        self.downloads_started_counter
            .add(1, &[KeyValue::new("type", kind.to_string())]);
        self.tcp_metrics.lock().unwrap().downloads_started += 1;
    }
    pub fn record_download_retry(&self, hash: impl Display) {
        debug!(name: "download_retry", hash = %hash);
        self.downloads_retry_counter.add(1, &[]);
        self.tcp_metrics.lock().unwrap().downloads_retry += 1;
    }

    pub fn update_download_progress(&self, hash: impl Display, newly_downloaded_bytes: u64) {
        self.downloads_bytes_counter.add(
            newly_downloaded_bytes,
            &[KeyValue::new("hash", hash.to_string())],
        );
        self.tcp_metrics.lock().unwrap().downloads_bytes += newly_downloaded_bytes;
    }

    pub fn record_download_completed(&self, hash: impl Display, from_peer: impl Display) {
        debug!(
            name:"download_complete",
            hash =%hash,
            from_peer =%from_peer
        );
        self.downloads_finished_counter.add(1, &[]);
        self.tcp_metrics.lock().unwrap().downloads_finished += 1;
    }

    pub fn record_download_failed(&self) {
        self.downloads_failed_counter.add(1, &[]);
        self.tcp_metrics.lock().unwrap().downloads_failed += 1;
    }
    pub fn record_download_perma_failed(&self) {
        self.downloads_perma_failed_counter.add(1, &[]);
        self.tcp_metrics.lock().unwrap().downloads_perma_failed += 1;
    }

    pub fn record_p2p_model_parameter_download_failed(&self) {
        self.record_download_perma_failed();
        self.p2p_params_download_failed_counter.add(1, &[]);
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
        self.tcp_metrics.lock().unwrap().connected_peers = connections.to_vec();

        // record connection counts by type
        for (conn_type, count) in connection_counts {
            self.peer_connections
                .record(count, &[KeyValue::new("connection_type", conn_type)]);
        }
    }

    pub fn update_p2p_gossip_neighbors(&self, neighbors: &[impl Display]) {
        let num_neighbors = neighbors.len() as u64;
        let neighbor_ids = neighbors.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        debug!(name: "gossip_neighbors", neighbors =  neighbor_ids.join(","));
        self.gossip_neighbors.record(num_neighbors, &[]);
        self.tcp_metrics.lock().unwrap().gossip_neighbors = neighbor_ids;
    }

    pub fn update_bandwidth(&self, bytes_per_second: f64) {
        self.bandwidth.record(bytes_per_second, &[]);
        self.tcp_metrics.lock().unwrap().bandwidth = bytes_per_second;
    }

    pub fn update_round_state(&self, step: u32, role: ClientRoleInRound) {
        self.round_step_gauge.record(step as u64, &[]);

        let participating = !matches!(role, ClientRoleInRound::NotInRound) as u64;

        {
            let mut metrics = self.tcp_metrics.lock().unwrap();
            metrics.round_step = step;
            metrics.role = role;
        }

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
    }

    pub fn initialize_model_parameters_gauge(&mut self, num_params: u64) {
        let meter = global::meter("psyche_client");
        self.num_params = Some(num_params);
        self.p2p_downloaded_params_percent = Some(meter
            .f64_gauge("psyche_p2p_model_params_downloaded")
            .with_description("Percentaje of the total model parameters that have been downloaded from other peers")
            .build());
    }

    pub fn update_model_sharing_total_params_downloaded(&mut self) {
        self.p2p_downloaded_params_total += 1;
        // TODO(marian): This should be changed and made more correct
        let total_params = self.num_params.unwrap();
        self.p2p_downloaded_params_percent.as_ref().map(|gauge| {
            gauge.record(
                self.p2p_downloaded_params_total as f64 / total_params as f64,
                &[],
            )
        });
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
                    if let Ok(device_count) = nvml.device_count() {
                        for i in 0..device_count {
                            if let Ok(gpu) = nvml.device_by_index(i) {
                                let device_info = [KeyValue::new("gpu", i as i64)];
                                if let Ok(util) = gpu.utilization_rates() {
                                    gpu_usage.record(util.gpu as f64, &device_info);
                                }
                                if let Ok(mem) = gpu.memory_info() {
                                    gpu_memory.record(mem.used, &device_info);
                                }
                                if let Ok(temp) = gpu.temperature(TemperatureSensor::Gpu) {
                                    gpu_temp.record(temp as u64, &device_info);
                                }
                            }
                        }
                    }
                }
                interval.tick().await;
            }
        }))
    }

    fn start_tcp_server(
        port: u16,
        tcp_metrics: Arc<Mutex<TcpMetrics>>,
    ) -> Arc<tokio::task::JoinHandle<()>> {
        Arc::new(tokio::spawn(async move {
            let addr = format!("127.0.0.1:{port}");
            let listener = match TcpListener::bind(&addr).await {
                Ok(listener) => listener,
                Err(e) => {
                    warn!(
                        "[metrics tcp server] Failed to bind TCP server on {}: {} -- Continuing without it",
                        addr, e
                    );
                    return;
                }
            };
            info!("[metrics tcp server] listening on {}", addr);

            loop {
                tokio::select! {
                    // when someone connects to us
                    Ok((mut stream, _)) = listener.accept() => {
                        // grab all the metrics
                        let stats_json = {
                            match serde_json::to_string(&*tcp_metrics.lock().unwrap()) {
                                Ok(json) => json,
                                Err(e) => {
                                    warn!("[metrics tcp server] Failed to serialize metrics: {}", e);
                                    continue;
                                }
                            }
                        };

                        // send metrics - we don't care if it fails
                        let _ = stream.write_all(stats_json.as_bytes()).await;
                        // and close the connection
                        let _ = stream.shutdown().await;
                    }
                }
            }
        }))
    }
}

impl Default for ClientMetrics {
    fn default() -> Self {
        Self::new(None)
    }
}
