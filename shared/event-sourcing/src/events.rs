use chrono::{DateTime, Utc};
use derive_more::Display;
use first_class_variants::first_class_variants;
use iroh::EndpointId;
use iroh_blobs::Hash as BlobHash;
use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::{ClientState, RunState};
use psyche_core::BatchId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum SubscriptionStatus {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum RpcCallType {
    Witness,
    WarmupWitness,
    HealthCheck,
    Checkpoint,
    Join,
    Tick,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: DateTime<Utc>,
    pub data: EventData,
}

#[derive(Debug, Clone, Serialize, Deserialize, Display)]
pub enum EventData {
    #[display("{_0}")]
    RunStarted(RunStarted),
    #[display("{_0}")]
    EpochStarted(EpochStarted),
    #[display("{_0}")]
    Coordinator(Coordinator),
    #[display("{_0}")]
    Client(Client),
    #[display("{_0}")]
    P2P(P2P),
    #[display("{_0}")]
    Train(Train),
    #[display("{_0}")]
    Warmup(Warmup),
    #[display("{_0}")]
    Cooldown(Cooldown),
    #[display("{_0}")]
    ResourceSnapshot(ResourceSnapshot),
}

#[derive(Debug, Clone, Serialize, Deserialize, Display)]
#[display("run started: node {node_id} version {psyche_version} run {run_id}")]
pub struct RunStarted {
    pub run_id: String,
    pub node_id: String,
    pub config: String,
    pub psyche_version: String,
}

impl From<RunStarted> for EventData {
    fn from(value: RunStarted) -> Self {
        EventData::RunStarted(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Display)]
#[display("epoch {epoch_number} started")]
pub struct EpochStarted {
    pub epoch_number: u64,
}

impl From<EpochStarted> for EventData {
    fn from(value: EpochStarted) -> Self {
        EventData::EpochStarted(value)
    }
}

#[first_class_variants(
    module = "coordinator",
    impl_into_parent = "EventData",
    derive(Debug, Clone, Serialize, Deserialize, Display)
)]
#[derive(Debug, Clone, Serialize, Deserialize, Display)]
pub enum Coordinator {
    #[display("coordinator state changed: {new_state_hash}")]
    CoordinatorStateChanged { new_state_hash: String },
    #[display("solana subscription {url}: {status}")]
    SolanaSubscriptionChanged {
        url: String,
        status: SubscriptionStatus,
    },
    #[display("rpc submitted: {call_type}")]
    RpcCallSubmitted { call_type: RpcCallType },
    #[display("rpc result: {call_type} success={success}")]
    RpcCallResult {
        call_type: RpcCallType,
        success: bool,
        error_string: Option<String>,
    },
}

#[first_class_variants(
    module = "client",
    impl_into_parent = "EventData",
    derive(Debug, Clone, Serialize, Deserialize, Display)
)]
#[derive(Debug, Clone, Serialize, Deserialize, Display)]
pub enum Client {
    #[display("state changed {old_state:?}→{new_state:?} epoch={epoch} step={step}")]
    StateChanged {
        old_state: RunState,
        new_state: RunState,
        epoch: u64,
        step: u64,
    },

    #[display("health check failed index={index} step={current_step}")]
    HealthCheckFailed { index: u64, current_step: u64 },

    #[display("{message}")]
    Error { message: String },

    #[display("{message}")]
    Warning { message: String },
}

#[first_class_variants(
    module = "p2p",
    impl_into_parent = "EventData",
    derive(Debug, Clone, Serialize, Deserialize, Display)
)]
#[derive(Debug, Clone, Serialize, Deserialize, Display)]
pub enum P2P {
    #[display("connection changed")]
    ConnectionChanged {
        endpoint_id: EndpointId,
        connection_path: Option<psyche_metrics::SelectedPath>,
    },
    #[display("latency to {endpoint_id} changed: {latency_ms}ms")]
    ConnectionLatencyChanged {
        endpoint_id: EndpointId,
        latency_ms: u64,
    },
    #[display("gossip neighbor up: {endpoint_id}")]
    GossipNeighborUp { endpoint_id: EndpointId },
    #[display("gossip neighbor down: {endpoint_id}")]
    GossipNeighborDown { endpoint_id: EndpointId },
    #[display("gossip sent: training result")]
    GossipTrainingResultSent,
    #[display("gossip sent: finished")]
    GossipFinishedSent,
    #[display("gossip received: training result {blob} batch={batch_id}")]
    GossipTrainingResultReceived { blob: BlobHash, batch_id: BatchId },
    #[display("gossip received: finished")]
    GossipFinishedReceived,
    #[display("gossip lagged")]
    GossipLagged,
    #[display("blob made available for upload: {blob}")]
    BlobAddedToStore {
        blob: BlobHash,
        model_parameter: String,
    },
    #[display("blob upload started: {size_bytes}B")]
    BlobUploadStarted {
        to_endpoint_id: EndpointId,
        size_bytes: u64,
    },
    #[display("blob upload progress: {bytes_transferred}B")]
    BlobUploadProgress { bytes_transferred: u64 },
    #[display("blob upload completed")]
    BlobUploadCompleted(Result<(), String>),

    #[display("blob download requested: {blob}")]
    BlobDownloadRequested { blob: BlobHash },
    #[display("blob download trying provider: {blob} {endpoint_id}")]
    BlobDownloadTryProvider {
        blob: BlobHash,
        endpoint_id: EndpointId,
    },
    #[display("blob download provider failed: {blob} {endpoint_id}")]
    BlobDownloadProviderFailed {
        blob: BlobHash,
        endpoint_id: EndpointId,
    },
    #[display("blob download started: {blob} {size_bytes}B")]
    BlobDownloadStarted { blob: BlobHash, size_bytes: u64 },
    #[display("blob download progress: {blob} {bytes_transferred}B")]
    BlobDownloadProgress {
        blob: BlobHash,
        bytes_transferred: u64,
    },
    #[display("blob download completed: {blob} success={success}")]
    BlobDownloadCompleted {
        blob: BlobHash,
        success: bool,
        error_string: Option<String>,
    },
}

#[first_class_variants(
    module = "train",
    impl_into_parent = "EventData",
    derive(Debug, Clone, Serialize, Deserialize, Display)
)]
#[derive(Debug, Clone, Serialize, Deserialize, Display)]
pub enum Train {
    #[display("batch assigned: {batch_id}")]
    BatchAssigned { batch_id: BatchId },
    #[display("batch data download start")]
    BatchDataDownloadStart,
    #[display("batch data download finished: success={}", result.is_ok())]
    BatchDataDownloadComplete { result: Result<(), ()> },

    #[display("training started: {batch_id}")]
    TrainingStarted { batch_id: BatchId },
    #[display("training finished: {batch_id} step={step} loss={loss:?}")]
    TrainingFinished {
        batch_id: BatchId,
        step: u64,
        loss: Option<f64>,
    },
    #[display("WARNING: untrained batch {batch_id}")]
    UntrainedBatchWarning {
        batch_id: BatchId,
        expected_trainer: Option<String>,
    },
    #[display("witness elected: step={step} round={round} epoch={epoch} witness={is_witness}")]
    WitnessElected {
        step: u64,
        round: u64,
        epoch: u64,
        index: u64,
        committee_position: u64,
        /// Whether this node was actually selected as a witness for this step.
        is_witness: bool,
    },

    #[display("distro result deserialize started: {blob}")]
    DistroResultDeserializeStarted { blob: BlobHash },
    #[display("distro result deserialize complete: {blob}")]
    DistroResultDeserializeComplete {
        blob: BlobHash,
        result: Result<(), String>,
    },
    #[display("apply distro results start")]
    ApplyDistroResultsStart,
    #[display("apply distro results complete")]
    ApplyDistroResultsComplete(Result<(), String>),
    #[display("distro result added to consensus")]
    DistroResultAddedToConsensus(Result<(), String>),
}

#[first_class_variants(
    module = "warmup",
    impl_into_parent = "EventData",
    derive(Debug, Clone, Serialize, Deserialize, Display)
)]
#[derive(Debug, Clone, Serialize, Deserialize, Display)]
pub enum Warmup {
    #[display("p2p param info request")]
    P2PParamInfoRequest { from: EndpointId },
    #[display("p2p param info response")]
    P2PParamInfoResponse,

    #[display("checkpoint download started: {size_bytes}B")]
    CheckpointDownloadStarted { size_bytes: u64 },
    #[display("checkpoint download progress: {bytes_downloaded}B")]
    CheckpointDownloadProgress { bytes_downloaded: u64 },
    #[display("checkpoint download complete")]
    CheckpointDownloadComplete(Result<(), String>),
    #[display("model load started")]
    ModelLoadStarted,
    #[display("model load complete")]
    ModelLoadComplete,
}

#[first_class_variants(
    module = "cooldown",
    impl_into_parent = "EventData",
    derive(Debug, Clone, Serialize, Deserialize, Display)
)]
#[derive(Debug, Clone, Serialize, Deserialize, Display)]
pub enum Cooldown {
    #[display("model serialization started")]
    ModelSerializationStarted,
    #[display("model serialization finished: success={success}")]
    ModelSerializationFinished {
        success: bool,
        error_string: Option<String>,
    },

    #[display("checkpoint write started")]
    CheckpointWriteStarted,
    #[display("checkpoint write finished: success={success}")]
    CheckpointWriteFinished {
        success: bool,
        error_string: Option<String>,
    },

    #[display("checkpoint upload started")]
    CheckpointUploadStarted,
    #[display("checkpoint upload progress: {bytes_uploaded}B")]
    CheckpointUploadProgress { bytes_uploaded: u64 },
    #[display("checkpoint upload finished: success={success}")]
    CheckpointUploadFinished {
        success: bool,
        error_string: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Display)]
#[display("resource snapshot: cpu={cpu_mem_used_bytes}B gpu={gpu_mem_used_bytes:?}B")]
pub struct ResourceSnapshot {
    pub gpu_mem_used_bytes: Option<u64>,
    pub gpu_utilization_percent: Option<f32>,
    pub cpu_mem_used_bytes: u64,
    pub cpu_utilization_percent: f32,
    pub network_bytes_sent_total: u64,
    pub network_bytes_recv_total: u64,
    pub disk_space_available_bytes: u64,
}

impl From<ResourceSnapshot> for EventData {
    fn from(value: ResourceSnapshot) -> Self {
        EventData::ResourceSnapshot(value)
    }
}

// ── Coordinator disk records ───────────────────────────────────────────────────
//
// Written by the coordinator server to `{events_dir}/coordinator/events.postcard`
// in COBS-framed postcard format — the same encoding used for node event files.
// Read by the observer via `ClusterTimeline::from_events_dir`.

/// A coordinator state snapshot written to disk on every state change.
#[derive(Debug, Clone, Serialize, Deserialize, Display)]
#[display("coordinator: epoch={epoch} step={step} state={run_state:?}")]
pub struct CoordinatorRecord {
    pub timestamp: DateTime<Utc>,
    pub run_state: RunState,
    pub epoch: u16,
    pub step: u32,
    pub checkpoint: Checkpoint,
    pub min_clients: u16,
    /// Active clients at the time of this snapshot.
    pub clients: Vec<CoordinatorClientRecord>,
    /// Batch → assigned node-id string for the current step.
    pub batch_assignments: std::collections::BTreeMap<BatchId, String>,
}

/// A single active client entry within a [`CoordinatorRecord`].
#[derive(Debug, Clone, Serialize, Deserialize, Display)]
#[display("{id}: {state:?}")]
pub struct CoordinatorClientRecord {
    /// Display string of the client's node identity.
    /// Matches the directory name used for that node's event files.
    pub id: String,
    pub state: ClientState,
}
