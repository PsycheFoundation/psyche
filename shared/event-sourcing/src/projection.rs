use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use iroh::EndpointId;
use iroh_blobs::Hash as BlobHash;
use psyche_coordinator::{RunState, model::Checkpoint};
use psyche_core::BatchId;
use psyche_metrics::SelectedPath;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::events::{
    Client, Cooldown, Coordinator, Event, EventData, P2P, ResourceSnapshot, RpcCallType, Train,
    Warmup,
};

// ── Coordinator ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CoordinatorStateSnapshot {
    pub timestamp: DateTime<Utc>,
    pub run_state: RunState,
    pub epoch: u64,
    pub step: u64,
    pub checkpoint: Checkpoint,
    pub client_ids: Vec<String>,
    pub min_clients: usize,
    /// Batch → node_id assignments for the current step.
    /// Populated by the coordinator source using `assign_data_for_state`.
    pub batch_assignments: BTreeMap<BatchId, String>,
}

// ── Warmup ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum WarmupPhase {
    #[default]
    Idle,
    NegotiatingP2P,
    Downloading,
    LoadingModel,
    Complete,
}

impl std::fmt::Display for WarmupPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarmupPhase::Idle => write!(f, "Idle"),
            WarmupPhase::NegotiatingP2P => write!(f, "Negotiating P2P"),
            WarmupPhase::Downloading => write!(f, "Downloading"),
            WarmupPhase::LoadingModel => write!(f, "Loading Model"),
            WarmupPhase::Complete => write!(f, "Complete"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WarmupSnapshot {
    pub phase: WarmupPhase,
    pub download_total_bytes: Option<u64>,
    pub download_bytes: u64,
    pub model_loaded: bool,
}

// ── P2P ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Whether we're talking to this peer directly or via a relay.
    pub connection_path: Option<SelectedPath>,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct BlobUploadTransfer {
    /// Endpoint we're uploading to.
    pub peer_endpoint_id: EndpointId,
    pub size_bytes: u64,
    pub bytes_transferred: u64,
    /// None = in progress, Some(Ok(())) = success, Some(Err(s)) = failed.
    pub result: Option<Result<(), String>>,
}

#[derive(Debug, Clone)]
pub struct BlobDownloadTransfer {
    pub size_bytes: u64,
    pub bytes_transferred: u64,
    /// None = in progress, Some(Ok(())) = success, Some(Err(s)) = failed.
    pub result: Option<Result<(), String>>,
}

#[derive(Debug, Clone, Default)]
pub struct P2PSnapshot {
    /// Currently known peers: iroh endpoint id → connection state.
    pub peers: IndexMap<EndpointId, PeerInfo>,
    /// Current gossip neighbourhood.
    pub gossip_neighbors: HashSet<EndpointId>,
    /// Blob uploads keyed by blob hash.
    pub uploads: IndexMap<BlobHash, BlobUploadTransfer>,
    /// Blob downloads keyed by blob hash.
    pub downloads: IndexMap<BlobHash, BlobDownloadTransfer>,
    /// Total number of blobs ever added to local iroh store.
    pub blobs_in_store: usize,
    /// Total gossip messages sent this session.
    pub gossip_sent: u32,
    /// Total gossip messages received this session.
    pub gossip_recv: u32,
}

// ── Train ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BatchDownload {
    pub result: Option<Result<(), ()>>,
}

#[derive(Debug, Clone)]
pub struct WitnessInfo {
    pub step: u64,
    pub round: u64,
    pub epoch: u64,
    pub index: u64,
    pub committee_position: u64,
}
#[derive(Debug, Clone, Default)]
pub struct TrainSnapshot {
    /// Number of batches assigned to this node for the current step.
    pub batches_assigned: u64,
    /// Per-batch download status for batches we're responsible for.
    pub batch_downloads: IndexMap<BatchId, BatchDownload>,
    /// True between TrainingStarted and TrainingFinished.
    pub training_in_progress: bool,
    /// Our witness election info for this step, if we were elected.
    pub witness: Option<WitnessInfo>,
    /// Whether the most recent DistroResult apply succeeded.
    pub last_distro_ok: Option<bool>,
    /// Batches we were warned about (batch_id, expected_trainer).
    pub untrained_warnings: Vec<(BatchId, Option<String>)>,
}

// ── Cooldown ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct CooldownSnapshot {
    /// True once we see ModelSerializationStarted — we are the checkpointer this round.
    pub is_checkpointer: bool,
    pub serialization_ok: Option<bool>,
    pub serialization_error: Option<String>,
    pub checkpoint_write_ok: Option<bool>,
    pub upload_bytes: u64,
    pub upload_ok: Option<bool>,
    pub upload_error: Option<String>,
}

// ── Cluster-level batch view ──────────────────────────────────────────────────

/// Per-node status for a batch's distro result propagation across the cluster.
#[derive(Debug, Clone, Default)]
pub struct NodeBatchStatus {
    /// Received gossip announcing this batch's training result.
    pub gossip_received: bool,
    /// Blob download: None = not started, Some(None) = in progress, Some(Some(ok)) = done.
    pub download: Option<Option<bool>>,
    /// Deserialized: None = not started, Some(ok) = done.
    pub deserialized: Option<bool>,
}

/// Witness election + RPC submission status for a single witness node.
#[derive(Debug, Clone)]
pub struct WitnessStatus {
    pub info: WitnessInfo,
    /// Whether this witness has submitted their attestation via RPC.
    pub submitted: bool,
    /// RPC result: None = pending, Some(ok) = got result.
    pub rpc_result: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct ClusterBatchView {
    /// Node assigned this batch, from coordinator state.
    pub assigned_to: Option<String>,
    /// Blob hash for this batch's distro result (learned from GossipTrainingResultReceived).
    pub blob: Option<BlobHash>,
    /// Whether the assigned trainer downloaded the training data.
    pub data_downloaded: Option<bool>,
    /// Whether the assigned trainer finished training.
    pub trained: bool,
    /// Per non-trainer node: gossip / download / deserialization status.
    pub node_status: IndexMap<String, NodeBatchStatus>,
}

// ── NodeSnapshot ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NodeSnapshot {
    pub node_id: String,
    pub run_state: Option<RunState>,
    pub epoch: u64,
    pub step: u64,
    pub warmup: WarmupSnapshot,
    /// (step, loss) — one entry per TrainingFinished with a loss value.
    pub losses: Vec<(u64, f64)>,
    pub health_check_steps: Vec<u64>,
    pub last_error: Option<String>,
    /// Instantaneous TX throughput in bytes/sec (derived from consecutive ResourceSnapshots).
    pub network_tx_bps: Option<u64>,
    /// Instantaneous RX throughput in bytes/sec.
    pub network_rx_bps: Option<u64>,
    pub p2p: P2PSnapshot,
    pub train: TrainSnapshot,
    pub cooldown: CooldownSnapshot,
    /// Last ResourceSnapshot seen, used to compute network bps deltas.
    pub last_resource: Option<(DateTime<Utc>, ResourceSnapshot)>,
}

impl NodeSnapshot {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            run_state: None,
            epoch: 0,
            step: 0,
            warmup: WarmupSnapshot::default(),
            losses: Vec::new(),
            health_check_steps: Vec::new(),
            last_error: None,
            network_tx_bps: None,
            network_rx_bps: None,
            p2p: P2PSnapshot::default(),
            train: TrainSnapshot::default(),
            cooldown: CooldownSnapshot::default(),
            last_resource: None,
        }
    }
}

// ── ClusterSnapshot ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ClusterSnapshot {
    pub timestamp: DateTime<Utc>,
    pub coordinator: Option<CoordinatorStateSnapshot>,
    pub nodes: IndexMap<String, NodeSnapshot>,
    /// Cluster-level per-batch view for the current step, merged from coordinator
    /// assignments (assigned_to) and per-node events (downloads, trained_by).
    /// Cleared and re-seeded whenever the coordinator reports a new step.
    pub step_batches: BTreeMap<BatchId, ClusterBatchView>,
    /// Batch view from the previous step, retained when the coordinator advances.
    /// Nodes may still emit download/train events for these batches after the step
    /// has advanced (e.g. distro result application in progress).
    pub prev_step_batches: BTreeMap<BatchId, ClusterBatchView>,
    /// Blob hash → BatchId mapping for correlating download events to batches.
    pub blob_to_batch: HashMap<BlobHash, BatchId>,
    /// Per-node "applied distro results" flag for the current step.
    pub applied_by: HashSet<String>,
    /// Per-node "applied distro results" flag for the previous step.
    pub prev_applied_by: HashSet<String>,
    /// Witnesses for the current step: node_id → witness submission status.
    pub step_witnesses: IndexMap<String, WitnessStatus>,
}

impl ClusterSnapshot {
    pub fn new() -> Self {
        Self {
            timestamp: Utc::now(),
            coordinator: None,
            nodes: IndexMap::new(),
            step_batches: BTreeMap::new(),
            prev_step_batches: BTreeMap::new(),
            blob_to_batch: HashMap::new(),
            applied_by: HashSet::new(),
            prev_applied_by: HashSet::new(),
            step_witnesses: IndexMap::new(),
        }
    }
}

impl Default for ClusterSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

// ── ClusterProjection ─────────────────────────────────────────────────────────

pub struct ClusterProjection {
    snapshot: ClusterSnapshot,
}

impl ClusterProjection {
    pub fn new() -> Self {
        Self {
            snapshot: ClusterSnapshot::new(),
        }
    }

    pub fn from_snapshot(snapshot: ClusterSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn into_snapshot(self) -> ClusterSnapshot {
        self.snapshot
    }

    pub fn apply_node_event(&mut self, node_id: &str, event: &Event) {
        self.snapshot.timestamp = event.timestamp;

        // ── Phase 1: per-node mutations ───────────────────────────────────────
        // Scoped so the &mut NodeSnapshot borrow on self.snapshot.nodes is released
        // before phase 2 touches self.snapshot.step_batches.
        {
            let node = self
                .snapshot
                .nodes
                .entry(node_id.to_string())
                .or_insert_with(|| NodeSnapshot::new(node_id.to_string()));

            match &event.data {
                // ── Client ───────────────────────────────────────────────────
                EventData::Client(client) => match client {
                    Client::StateChanged(sc) => {
                        node.run_state = Some(sc.new_state);
                        node.epoch = sc.epoch;
                        node.step = sc.step;
                        if sc.new_state == RunState::WaitingForMembers {
                            node.warmup = WarmupSnapshot::default();
                        }
                        if sc.new_state == RunState::Cooldown {
                            node.cooldown = CooldownSnapshot::default();
                        }
                    }
                    Client::HealthCheckFailed(hcf) => {
                        node.health_check_steps.push(hcf.current_step);
                    }
                    Client::Error(e) => {
                        node.last_error = Some(e.message.clone());
                    }
                    Client::Warning(_) => {}
                },

                // ── Train ────────────────────────────────────────────────────
                EventData::Train(train) => match train {
                    Train::BatchAssigned(ba) => {
                        node.train.batches_assigned += 1;
                        node.train
                            .batch_downloads
                            .entry(ba.batch_id)
                            .or_insert(BatchDownload { result: None });
                        node.train.training_in_progress = false;
                        node.train.witness = None;
                    }
                    Train::BatchDataDownloadStart(_) => {}
                    Train::BatchDataDownloadComplete(c) => {
                        // Mark all pending downloads as complete
                        for (_, dl) in node.train.batch_downloads.iter_mut() {
                            if dl.result.is_none() {
                                dl.result = Some(c.result);
                            }
                        }
                    }
                    Train::TrainingStarted(_) => {
                        node.train.training_in_progress = true;
                    }
                    Train::TrainingFinished(crate::train::TrainingFinished {
                        batch_id: _,
                        step,
                        loss,
                    }) => {
                        node.train.training_in_progress = false;
                        if let Some(loss) = *loss {
                            node.losses.push((*step, loss));
                        }
                        node.step = *step;
                    }
                    Train::WitnessElected(we) => {
                        node.train.witness = Some(WitnessInfo {
                            step: we.step,
                            round: we.round,
                            epoch: we.epoch,
                            index: we.index,
                            committee_position: we.committee_position,
                        });
                    }
                    Train::UntrainedBatchWarning(ubw) => {
                        node.train
                            .untrained_warnings
                            .push((ubw.batch_id, ubw.expected_trainer.clone()));
                    }
                    Train::DistroResultDeserializeStarted(_)
                    | Train::DistroResultDeserializeComplete(_) => {}
                    Train::ApplyDistroResultsStart(_) => {
                        node.train.last_distro_ok = None;
                    }
                    Train::ApplyDistroResultsComplete(arc) => {
                        node.train.last_distro_ok = Some(arc.0.is_ok());
                    }
                    Train::DistroResultAddedToConsensus(_) => {}
                },

                // ── Warmup ───────────────────────────────────────────────────
                EventData::Warmup(warmup) => match warmup {
                    Warmup::P2PParamInfoRequest(_) | Warmup::P2PParamInfoResponse(_) => {
                        if node.warmup.phase == WarmupPhase::Idle {
                            node.warmup.phase = WarmupPhase::NegotiatingP2P;
                        }
                    }
                    Warmup::CheckpointDownloadStarted(cds) => {
                        node.warmup.phase = WarmupPhase::Downloading;
                        node.warmup.download_total_bytes = Some(cds.size_bytes);
                        node.warmup.download_bytes = 0;
                    }
                    Warmup::CheckpointDownloadProgress(cdp) => {
                        node.warmup.download_bytes = cdp.bytes_downloaded;
                    }
                    Warmup::CheckpointDownloadComplete(_) => {}
                    Warmup::ModelLoadStarted(_) => {
                        node.warmup.phase = WarmupPhase::LoadingModel;
                    }
                    Warmup::ModelLoadComplete(_) => {
                        node.warmup.phase = WarmupPhase::Complete;
                        node.warmup.model_loaded = true;
                    }
                },

                // ── Cooldown ─────────────────────────────────────────────────
                EventData::Cooldown(cooldown) => match cooldown {
                    Cooldown::ModelSerializationStarted(_) => {
                        node.cooldown.is_checkpointer = true;
                    }
                    Cooldown::ModelSerializationFinished(msf) => {
                        node.cooldown.serialization_ok = Some(msf.success);
                        node.cooldown.serialization_error = msf.error_string.clone();
                    }
                    Cooldown::CheckpointWriteStarted(_) => {}
                    Cooldown::CheckpointWriteFinished(cwf) => {
                        node.cooldown.checkpoint_write_ok = Some(cwf.success);
                    }
                    Cooldown::CheckpointUploadStarted(_) => {}
                    Cooldown::CheckpointUploadProgress(cup) => {
                        node.cooldown.upload_bytes = cup.bytes_uploaded;
                    }
                    Cooldown::CheckpointUploadFinished(cuf) => {
                        node.cooldown.upload_ok = Some(cuf.success);
                        node.cooldown.upload_error = cuf.error_string.clone();
                        if cuf.success {
                            node.warmup = WarmupSnapshot::default();
                        }
                    }
                },

                // ── P2P ──────────────────────────────────────────────────────
                EventData::P2P(p2p) => match p2p {
                    P2P::ConnectionChanged(cc) => {
                        let existing_latency = node
                            .p2p
                            .peers
                            .get(&cc.endpoint_id)
                            .and_then(|p| p.latency_ms);
                        node.p2p.peers.insert(
                            cc.endpoint_id,
                            PeerInfo {
                                connection_path: cc.connection_path.clone(),
                                latency_ms: existing_latency,
                            },
                        );
                    }
                    P2P::GossipNeighborUp(gnu) => {
                        node.p2p.gossip_neighbors.insert(gnu.endpoint_id);
                    }
                    P2P::GossipNeighborDown(gnd) => {
                        node.p2p.gossip_neighbors.remove(&gnd.endpoint_id);
                    }
                    P2P::GossipLagged(_) => {}
                    P2P::ConnectionLatencyChanged(clc) => {
                        if let Some(peer) = node.p2p.peers.get_mut(&clc.endpoint_id) {
                            peer.latency_ms = Some(clc.latency_ms);
                        }
                    }
                    P2P::BlobAddedToStore(_) => {
                        node.p2p.blobs_in_store += 1;
                    }
                    P2P::BlobUploadStarted(crate::p2p::BlobUploadStarted {
                        to_endpoint_id,
                        size_bytes,
                    }) => {
                        // Upload events don't carry a blob hash yet (iroh doesn't
                        // expose upload-side progress); skip tracking for now.
                        let _ = (to_endpoint_id, size_bytes);
                    }
                    P2P::BlobUploadProgress(_) | P2P::BlobUploadCompleted(_) => {}
                    P2P::BlobDownloadStarted(bds) => {
                        node.p2p.downloads.insert(
                            bds.blob,
                            BlobDownloadTransfer {
                                size_bytes: bds.size_bytes,
                                bytes_transferred: 0,
                                result: None,
                            },
                        );
                    }
                    P2P::BlobDownloadProgress(bdp) => {
                        if let Some(t) = node.p2p.downloads.get_mut(&bdp.blob) {
                            t.bytes_transferred = bdp.bytes_transferred;
                        }
                    }
                    P2P::BlobDownloadCompleted(bdc) => {
                        if let Some(t) = node.p2p.downloads.get_mut(&bdc.blob) {
                            t.result = Some(if bdc.success {
                                Ok(())
                            } else {
                                Err(bdc.error_string.clone().unwrap_or_default())
                            });
                        }
                    }
                    P2P::GossipTrainingResultSent(_) | P2P::GossipFinishedSent(_) => {
                        node.p2p.gossip_sent += 1;
                    }
                    P2P::GossipTrainingResultReceived(_) | P2P::GossipFinishedReceived(_) => {
                        node.p2p.gossip_recv += 1;
                    }
                    P2P::BlobDownloadRequested(_)
                    | P2P::BlobDownloadTryProvider(_)
                    | P2P::BlobDownloadProviderFailed(_) => {}
                },

                // ── ResourceSnapshot ─────────────────────────────────────────
                EventData::ResourceSnapshot(rs) => {
                    if let Some((prev_ts, ref prev_rs)) = node.last_resource {
                        let dt_secs = (event.timestamp - prev_ts).num_milliseconds() as u64 / 1000;
                        if dt_secs > 0 {
                            node.network_tx_bps = Some(
                                rs.network_bytes_sent_total
                                    .saturating_sub(prev_rs.network_bytes_sent_total)
                                    / dt_secs,
                            );
                            node.network_rx_bps = Some(
                                rs.network_bytes_recv_total
                                    .saturating_sub(prev_rs.network_bytes_recv_total)
                                    / dt_secs,
                            );
                        }
                    }
                    node.last_resource = Some((event.timestamp, rs.clone()));
                }

                _ => {}
            }
        }

        // ── Phase 2: cluster-level step_batches updates ───────────────────────

        match &event.data {
            EventData::Train(Train::BatchDataDownloadComplete(c)) => {
                // Mark data_downloaded on all batches assigned to this node.
                let ok = c.result.is_ok();
                for (_, view) in self.snapshot.step_batches.iter_mut() {
                    if view.assigned_to.as_deref() == Some(node_id)
                        && view.data_downloaded.is_none()
                    {
                        view.data_downloaded = Some(ok);
                    }
                }
                for (_, view) in self.snapshot.prev_step_batches.iter_mut() {
                    if view.assigned_to.as_deref() == Some(node_id)
                        && view.data_downloaded.is_none()
                    {
                        view.data_downloaded = Some(ok);
                    }
                }
            }
            EventData::Train(Train::TrainingFinished(tf)) => {
                let batch_id = tf.batch_id;
                let map = if self.in_prev(batch_id) {
                    &mut self.snapshot.prev_step_batches
                } else {
                    &mut self.snapshot.step_batches
                };
                if let Some(view) = map.get_mut(&batch_id) {
                    view.trained = true;
                }
            }
            EventData::Train(Train::ApplyDistroResultsComplete(arc)) => {
                // Apply is step-level: mark this node as having applied.
                if arc.0.is_ok() {
                    self.snapshot.applied_by.insert(node_id.to_string());
                }
            }
            EventData::Train(Train::WitnessElected(we)) => {
                if we.is_witness {
                    self.snapshot.step_witnesses.insert(
                        node_id.to_string(),
                        WitnessStatus {
                            info: WitnessInfo {
                                step: we.step,
                                round: we.round,
                                epoch: we.epoch,
                                index: we.index,
                                committee_position: we.committee_position,
                            },
                            submitted: false,
                            rpc_result: None,
                        },
                    );
                }
            }
            EventData::Train(Train::DistroResultDeserializeComplete(drc)) => {
                if let Some(&batch_id) = self.snapshot.blob_to_batch.get(&drc.blob) {
                    let map = if self.in_prev(batch_id) {
                        &mut self.snapshot.prev_step_batches
                    } else {
                        &mut self.snapshot.step_batches
                    };
                    if let Some(view) = map.get_mut(&batch_id) {
                        view.node_status
                            .entry(node_id.to_string())
                            .or_default()
                            .deserialized = Some(drc.result.is_ok());
                    }
                }
            }
            EventData::P2P(P2P::GossipTrainingResultReceived(gtr)) => {
                let batch_id = gtr.batch_id;
                self.snapshot.blob_to_batch.insert(gtr.blob, batch_id);
                let map = if self.in_prev(batch_id) {
                    &mut self.snapshot.prev_step_batches
                } else {
                    &mut self.snapshot.step_batches
                };
                if let Some(view) = map.get_mut(&batch_id) {
                    if view.blob.is_none() {
                        view.blob = Some(gtr.blob);
                    }
                    view.node_status
                        .entry(node_id.to_string())
                        .or_default()
                        .gossip_received = true;
                }
            }
            EventData::P2P(P2P::BlobDownloadStarted(bds)) => {
                if let Some(&batch_id) = self.snapshot.blob_to_batch.get(&bds.blob) {
                    let map = if self.in_prev(batch_id) {
                        &mut self.snapshot.prev_step_batches
                    } else {
                        &mut self.snapshot.step_batches
                    };
                    if let Some(view) = map.get_mut(&batch_id) {
                        view.node_status
                            .entry(node_id.to_string())
                            .or_default()
                            .download = Some(None);
                    }
                }
            }
            EventData::P2P(P2P::BlobDownloadCompleted(bdc)) => {
                if let Some(&batch_id) = self.snapshot.blob_to_batch.get(&bdc.blob) {
                    let map = if self.in_prev(batch_id) {
                        &mut self.snapshot.prev_step_batches
                    } else {
                        &mut self.snapshot.step_batches
                    };
                    if let Some(view) = map.get_mut(&batch_id) {
                        view.node_status
                            .entry(node_id.to_string())
                            .or_default()
                            .download = Some(Some(bdc.success));
                    }
                }
            }
            EventData::Coordinator(Coordinator::RpcCallSubmitted(sub)) => {
                if matches!(
                    sub.call_type,
                    RpcCallType::Witness | RpcCallType::WarmupWitness
                ) {
                    if let Some(ws) = self.snapshot.step_witnesses.get_mut(node_id) {
                        ws.submitted = true;
                    }
                }
            }
            EventData::Coordinator(Coordinator::RpcCallResult(res)) => {
                if matches!(
                    res.call_type,
                    RpcCallType::Witness | RpcCallType::WarmupWitness
                ) {
                    if let Some(ws) = self.snapshot.step_witnesses.get_mut(node_id) {
                        ws.rpc_result = Some(res.success);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn apply_coordinator(&mut self, update: CoordinatorStateSnapshot) {
        self.snapshot.timestamp = update.timestamp;

        // If the step changed, clear stale batch data and re-seed from new assignments.
        let step_changed = self.snapshot.coordinator.as_ref().map(|c| c.step) != Some(update.step);
        if step_changed {
            // Preserve the outgoing step's batch data — nodes may still emit events
            // (distro result downloads, late TrainingFinished) after the coordinator
            // has advanced to the new step.
            self.snapshot.prev_step_batches = std::mem::take(&mut self.snapshot.step_batches);
            self.snapshot.prev_applied_by = std::mem::take(&mut self.snapshot.applied_by);
            self.snapshot.blob_to_batch.clear();
            self.snapshot.step_witnesses.clear();
            for (batch_id, node_id) in &update.batch_assignments {
                self.snapshot.step_batches.insert(
                    *batch_id,
                    ClusterBatchView {
                        assigned_to: Some(node_id.clone()),
                        ..Default::default()
                    },
                );
            }
        } else {
            // Same step — just refresh assigned_to in case assignments arrived late.
            for (batch_id, node_id) in &update.batch_assignments {
                self.snapshot
                    .step_batches
                    .entry(*batch_id)
                    .or_default()
                    .assigned_to = Some(node_id.clone());
            }
        }

        self.snapshot.coordinator = Some(update);
    }

    pub fn snapshot(&self) -> &ClusterSnapshot {
        &self.snapshot
    }

    // Returns true if the batch_id belongs to the previous step's map
    // (i.e. it's not in the current step but IS in the previous step).
    fn in_prev(&self, batch_id: BatchId) -> bool {
        !self.snapshot.step_batches.contains_key(&batch_id)
            && self.snapshot.prev_step_batches.contains_key(&batch_id)
    }
}

impl Default for ClusterProjection {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventData;
    use crate::{client, cooldown, p2p, train, warmup};
    use chrono::Utc;

    fn make_event(data: EventData) -> Event {
        Event {
            timestamp: Utc::now(),
            data,
        }
    }

    #[test]
    fn test_warmup_phase_transitions() {
        let mut proj = ClusterProjection::new();
        let node_id = "node-1";

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Warmup(
                crate::events::Warmup::P2PParamInfoResponse(warmup::P2PParamInfoResponse),
            )),
        );
        assert_eq!(
            proj.snapshot().nodes[node_id].warmup.phase,
            WarmupPhase::NegotiatingP2P
        );

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Warmup(
                crate::events::Warmup::CheckpointDownloadStarted(
                    warmup::CheckpointDownloadStarted { size_bytes: 1024 },
                ),
            )),
        );
        assert_eq!(
            proj.snapshot().nodes[node_id].warmup.phase,
            WarmupPhase::Downloading
        );
        assert_eq!(
            proj.snapshot().nodes[node_id].warmup.download_total_bytes,
            Some(1024)
        );

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Warmup(
                crate::events::Warmup::CheckpointDownloadProgress(
                    warmup::CheckpointDownloadProgress {
                        bytes_downloaded: 512,
                    },
                ),
            )),
        );
        assert_eq!(proj.snapshot().nodes[node_id].warmup.download_bytes, 512);

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Warmup(crate::events::Warmup::ModelLoadStarted(
                warmup::ModelLoadStarted,
            ))),
        );
        assert_eq!(
            proj.snapshot().nodes[node_id].warmup.phase,
            WarmupPhase::LoadingModel
        );

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Warmup(crate::events::Warmup::ModelLoadComplete(
                warmup::ModelLoadComplete,
            ))),
        );
        assert_eq!(
            proj.snapshot().nodes[node_id].warmup.phase,
            WarmupPhase::Complete
        );
        assert!(proj.snapshot().nodes[node_id].warmup.model_loaded);
    }

    #[test]
    fn test_state_changed_updates_node() {
        let mut proj = ClusterProjection::new();
        let node_id = "node-2";

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Client(crate::events::Client::StateChanged(
                client::StateChanged {
                    old_state: RunState::Uninitialized,
                    new_state: RunState::RoundTrain,
                    epoch: 2,
                    step: 7,
                },
            ))),
        );

        let node = &proj.snapshot().nodes[node_id];
        assert_eq!(node.run_state, Some(RunState::RoundTrain));
        assert_eq!(node.epoch, 2);
        assert_eq!(node.step, 7);
    }

    #[test]
    fn test_training_finished_records_loss() {
        let mut proj = ClusterProjection::new();
        let node_id = "node-3";

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Train(crate::events::Train::TrainingFinished(
                train::TrainingFinished {
                    batch_id: BatchId(psyche_core::ClosedInterval { start: 0, end: 0 }),
                    step: 5,
                    loss: Some(2.5),
                },
            ))),
        );

        let node = &proj.snapshot().nodes[node_id];
        assert_eq!(node.losses, vec![(5, 2.5)]);
        assert_eq!(node.step, 5);
    }

    #[test]
    fn test_warmup_resets_on_waiting_for_members() {
        let mut proj = ClusterProjection::new();
        let node_id = "node-4";

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Warmup(crate::events::Warmup::ModelLoadComplete(
                warmup::ModelLoadComplete,
            ))),
        );
        assert_eq!(
            proj.snapshot().nodes[node_id].warmup.phase,
            WarmupPhase::Complete
        );

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Client(crate::events::Client::StateChanged(
                client::StateChanged {
                    old_state: RunState::Cooldown,
                    new_state: RunState::WaitingForMembers,
                    epoch: 2,
                    step: 0,
                },
            ))),
        );
        assert_eq!(
            proj.snapshot().nodes[node_id].warmup.phase,
            WarmupPhase::Idle
        );
    }

    #[test]
    fn test_batch_assigned_tracked() {
        let mut proj = ClusterProjection::new();
        let node_id = "node-5";
        let batch_id = BatchId(psyche_core::ClosedInterval { start: 0, end: 9 });

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Train(crate::events::Train::BatchAssigned(
                train::BatchAssigned { batch_id },
            ))),
        );

        let node = &proj.snapshot().nodes[node_id];
        assert!(node.train.batch_downloads.contains_key(&batch_id));
        assert_eq!(node.train.batches_assigned, 1);
    }

    #[test]
    fn test_cooldown_checkpointer_flag() {
        let mut proj = ClusterProjection::new();
        let node_id = "node-6";

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Client(crate::events::Client::StateChanged(
                client::StateChanged {
                    old_state: RunState::RoundWitness,
                    new_state: RunState::Cooldown,
                    epoch: 1,
                    step: 10,
                },
            ))),
        );
        assert!(!proj.snapshot().nodes[node_id].cooldown.is_checkpointer);

        proj.apply_node_event(
            node_id,
            &make_event(EventData::Cooldown(
                crate::events::Cooldown::ModelSerializationStarted(
                    cooldown::ModelSerializationStarted,
                ),
            )),
        );
        assert!(proj.snapshot().nodes[node_id].cooldown.is_checkpointer);
    }

    #[test]
    fn test_p2p_gossip_neighbors() {
        use crate::events::p2p;

        let mut proj = ClusterProjection::new();
        let node_id = "node-7";

        let ep1 = iroh::SecretKey::generate(&mut rand::rng()).public();
        let ep2 = iroh::SecretKey::generate(&mut rand::rng()).public();

        proj.apply_node_event(
            node_id,
            &make_event(EventData::P2P(crate::events::P2P::GossipNeighborUp(
                p2p::GossipNeighborUp { endpoint_id: ep1 },
            ))),
        );
        proj.apply_node_event(
            node_id,
            &make_event(EventData::P2P(crate::events::P2P::GossipNeighborUp(
                p2p::GossipNeighborUp { endpoint_id: ep2 },
            ))),
        );

        let neighbors = &proj.snapshot().nodes[node_id].p2p.gossip_neighbors;
        assert!(neighbors.contains(&ep1));
        assert!(neighbors.contains(&ep2));

        proj.apply_node_event(
            node_id,
            &make_event(EventData::P2P(crate::events::P2P::GossipNeighborDown(
                p2p::GossipNeighborDown { endpoint_id: ep1 },
            ))),
        );

        let neighbors = &proj.snapshot().nodes[node_id].p2p.gossip_neighbors;
        assert!(!neighbors.contains(&ep1));
        assert!(neighbors.contains(&ep2));
    }

    #[test]
    fn test_coordinator_step_batches() {
        let mut proj = ClusterProjection::new();

        let b1 = BatchId(psyche_core::ClosedInterval { start: 0, end: 4 });
        let b2 = BatchId(psyche_core::ClosedInterval { start: 5, end: 9 });

        let mut assignments = BTreeMap::new();
        assignments.insert(b1, "node-A".to_string());
        assignments.insert(b2, "node-B".to_string());

        proj.apply_coordinator(CoordinatorStateSnapshot {
            timestamp: Utc::now(),
            run_state: RunState::RoundTrain,
            epoch: 0,
            step: 1,
            checkpoint: psyche_coordinator::model::Checkpoint::Ephemeral,
            client_ids: vec![],
            min_clients: 1,
            batch_assignments: assignments,
        });

        assert_eq!(
            proj.snapshot().step_batches[&b1].assigned_to.as_deref(),
            Some("node-A")
        );
        assert_eq!(
            proj.snapshot().step_batches[&b2].assigned_to.as_deref(),
            Some("node-B")
        );

        // New step clears old batch data
        proj.apply_coordinator(CoordinatorStateSnapshot {
            timestamp: Utc::now(),
            run_state: RunState::RoundTrain,
            epoch: 0,
            step: 2,
            checkpoint: psyche_coordinator::model::Checkpoint::Ephemeral,
            client_ids: vec![],
            min_clients: 1,
            batch_assignments: BTreeMap::new(),
        });

        assert!(proj.snapshot().step_batches.is_empty());
    }

    #[test]
    fn test_gossip_and_download_tracking() {
        let mut proj = ClusterProjection::new();
        let b1 = BatchId(psyche_core::ClosedInterval { start: 0, end: 4 });
        let blob = iroh_blobs::Hash::from_bytes([42u8; 32]);

        // Set up coordinator with a batch assigned to node-A.
        let mut assignments = BTreeMap::new();
        assignments.insert(b1, "node-A".to_string());
        proj.apply_coordinator(CoordinatorStateSnapshot {
            timestamp: Utc::now(),
            run_state: RunState::RoundTrain,
            epoch: 0,
            step: 1,
            checkpoint: psyche_coordinator::model::Checkpoint::Ephemeral,
            client_ids: vec![],
            min_clients: 1,
            batch_assignments: assignments,
        });

        // node-B receives gossip for this batch.
        proj.apply_node_event(
            "node-B",
            &make_event(EventData::P2P(
                crate::events::P2P::GossipTrainingResultReceived(
                    p2p::GossipTrainingResultReceived { blob, batch_id: b1 },
                ),
            )),
        );

        let view = &proj.snapshot().step_batches[&b1];
        assert_eq!(view.blob, Some(blob));
        assert!(view.node_status["node-B"].gossip_received);

        // node-B starts downloading the blob.
        proj.apply_node_event(
            "node-B",
            &make_event(EventData::P2P(crate::events::P2P::BlobDownloadStarted(
                p2p::BlobDownloadStarted {
                    blob,
                    size_bytes: 1024,
                },
            ))),
        );
        assert_eq!(
            proj.snapshot().step_batches[&b1].node_status["node-B"].download,
            Some(None)
        );

        // node-B completes download.
        proj.apply_node_event(
            "node-B",
            &make_event(EventData::P2P(crate::events::P2P::BlobDownloadCompleted(
                p2p::BlobDownloadCompleted {
                    blob,
                    success: true,
                    error_string: None,
                },
            ))),
        );
        assert_eq!(
            proj.snapshot().step_batches[&b1].node_status["node-B"].download,
            Some(Some(true))
        );

        // node-B deserializes it.
        proj.apply_node_event(
            "node-B",
            &make_event(EventData::Train(
                crate::events::Train::DistroResultDeserializeComplete(
                    train::DistroResultDeserializeComplete {
                        blob,
                        result: Ok(()),
                    },
                ),
            )),
        );
        assert_eq!(
            proj.snapshot().step_batches[&b1].node_status["node-B"].deserialized,
            Some(true)
        );

        // node-B applies distro results.
        proj.apply_node_event(
            "node-B",
            &make_event(EventData::Train(
                crate::events::Train::ApplyDistroResultsComplete(
                    train::ApplyDistroResultsComplete(Ok(())),
                ),
            )),
        );
        assert!(proj.snapshot().applied_by.contains("node-B"));
    }
}
