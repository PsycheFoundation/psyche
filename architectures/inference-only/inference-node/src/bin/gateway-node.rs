//! Gateway node for inference requests
//!
//! Usage:
//! 1. Exposes HTTP API on localhost:8000
//! 2. Discovers inference nodes via gossip
//! 3. Routes requests to available inference nodes via gossip
//! 4. Returns responses to HTTP clients
//!
//!   cargo run --bin gateway-node -- --discovery-mode local

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use clap::Parser;
use psyche_inference::{
    INFERENCE_ALPN, InferenceGossipMessage, InferenceMessage, InferenceRequest, InferenceResponse,
};
use psyche_metrics::ClientMetrics;
use psyche_network::{
    DiscoveryMode, EndpointId, NetworkConnection, NetworkEvent, RelayKind, allowlist,
};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    sync::{RwLock, mpsc},
    time::sleep,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Default path for storing model assignments
const ASSIGNMENTS_FILE: &str = "/tmp/psyche-gateway-assignments.json";

/// Load model assignments from disk
fn load_assignments(path: &str) -> HashMap<EndpointId, String> {
    match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<HashMap<EndpointId, String>>(&contents) {
            Ok(assignments) => {
                info!(
                    "Loaded {} model assignments from {}",
                    assignments.len(),
                    path
                );
                assignments
            }
            Err(e) => {
                warn!("Failed to parse assignments file: {:#}", e);
                HashMap::new()
            }
        },
        Err(_) => {
            info!("No assignments file found at {}, starting fresh", path);
            HashMap::new()
        }
    }
}

/// Save model assignments to disk
fn save_assignments(path: &str, assignments: &HashMap<EndpointId, String>) -> Result<()> {
    let json =
        serde_json::to_string_pretty(assignments).context("Failed to serialize assignments")?;
    fs::write(path, json).context("Failed to write assignments file")?;
    debug!("Saved {} model assignments to {}", assignments.len(), path);
    Ok(())
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "0.0.0.0:8000")]
    listen_addr: String,

    /// what discovery to use - public n0 or local
    #[arg(long, env = "IROH_DISCOVERY", default_value = "n0")]
    discovery_mode: DiscoveryMode,

    /// what relays to use - public n0 or the private Psyche ones
    #[arg(long, env = "IROH_RELAY", default_value = "psyche")]
    relay_kind: RelayKind,

    #[arg(long)]
    bootstrap_peer_file: Option<PathBuf>,

    #[arg(long)]
    write_endpoint_file: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct InferenceNodeInfo {
    peer_id: EndpointId,
    model_name: Option<String>,
    #[allow(dead_code)]
    checkpoint_id: Option<String>,
    #[allow(dead_code)]
    capabilities: Vec<String>,
}

struct GatewayState {
    available_nodes: RwLock<HashMap<EndpointId, InferenceNodeInfo>>,
    pending_requests: RwLock<HashMap<String, mpsc::Sender<InferenceResponse>>>,
    model_assignments: RwLock<HashMap<EndpointId, String>>, // node_id -> assigned model name
    network_tx: mpsc::Sender<(EndpointId, InferenceMessage)>,
    gossip_tx: mpsc::Sender<InferenceGossipMessage>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(serde::Deserialize)]
struct ChatCompletionRequest {
    model: Option<String>,
    messages: Vec<ChatMessage>,
    #[serde(default = "default_max_tokens")]
    max_tokens: Option<usize>,
    #[serde(default = "default_temperature")]
    temperature: Option<f64>,
    #[serde(default = "default_top_p")]
    top_p: Option<f64>,
    #[serde(default)]
    stream: bool,
}

fn default_max_tokens() -> Option<usize> {
    Some(100)
}
fn default_temperature() -> Option<f64> {
    Some(1.0)
}
fn default_top_p() -> Option<f64> {
    Some(1.0)
}

#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
enum ModelSourceType {
    #[default]
    HuggingFace,
    Local,
}

#[derive(serde::Deserialize)]
struct AssignModelsRequest {
    assignments: Vec<ModelAssignmentSpec>,
}

#[derive(serde::Deserialize)]
struct ModelAssignmentSpec {
    model_name: String,
    #[serde(default)]
    source_type: ModelSourceType,
    #[serde(default)]
    source_path: Option<String>,
    num_nodes: usize,
}

#[derive(serde::Serialize)]
struct AssignmentInfo {
    node_id: String,
    model_name: String,
    status: String, // "loading", "loaded", "idle", "offline"
}

fn default_model_source_type() -> String {
    "huggingface".to_string()
}

#[derive(serde::Serialize)]
struct LoadModelResponse {
    success: bool,
    message: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(tag = "source_type", rename_all = "lowercase")]
enum LoadModelSource {
    #[serde(rename = "huggingface")]
    HuggingFace {
        source_path: Option<String>,
    },
    Local {
        source_path: String,
    },
}

#[derive(serde::Deserialize)]
struct LoadModelRequest {
    model_name: String,
    #[serde(flatten)]
    source: LoadModelSource,
}

#[derive(serde::Serialize)]
struct ChatCompletionChoice {
    index: usize,
    message: ChatMessage,
    finish_reason: Option<String>,
}

#[derive(serde::Serialize)]
struct ChatCompletionResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<ChatCompletionChoice>,
    // we're omitting usage stats for now
}

#[axum::debug_handler]
async fn handle_inference(
    State(state): State<Arc<GatewayState>>,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, AppError> {
    let nodes = state.available_nodes.read().await;
    let assignments = state.model_assignments.read().await;

    // Determine requested model
    let requested_model = req.model.as_deref();

    // Find suitable nodes:
    // 1. If model specified: prefer nodes assigned to that model with it loaded
    // 2. If no model specified: use any node with a model loaded
    let suitable_nodes: Vec<_> = if let Some(model) = requested_model {
        // Prefer nodes assigned to the requested model that have it loaded
        let assigned_and_loaded: Vec<_> = nodes
            .values()
            .filter(|n| {
                assignments
                    .get(&n.peer_id)
                    .map(|assigned| assigned == model)
                    .unwrap_or(false)
                    && n.model_name.as_deref() == Some(model)
            })
            .collect();

        if !assigned_and_loaded.is_empty() {
            assigned_and_loaded
        } else {
            // Fallback: any node with the requested model loaded
            nodes
                .values()
                .filter(|n| n.model_name.as_deref() == Some(model))
                .collect()
        }
    } else {
        // No model specified - use any node with a model loaded
        nodes.values().filter(|n| n.model_name.is_some()).collect()
    };

    let nodes_with_model: Vec<(EndpointId, String)> = nodes
        .values()
        .filter_map(|n| Some((n.peer_id, n.model_name.clone()?)))
        .collect();

    if nodes_with_model.is_empty() {
        // No nodes have models loaded yet
        return Err(AppError::NoNodesAvailable);
    }

    // Select first available node with a model
    // TODO: Add load balancing and model-specific routing in the future
    let (target_peer_id, node_model_name) = &nodes_with_model[0];
    let target_peer_id = *target_peer_id;

    let model_name = req.model.clone().unwrap_or_else(|| node_model_name.clone());

    info!(
        "Routing request to node: {} (model: {}, assigned: {})",
        target_peer_id.fmt_short(),
        node.model_name.as_deref().unwrap_or("unknown"),
        assignments
            .get(&target_peer_id)
            .map(|s| s.as_str())
            .unwrap_or("none")
    );
    drop(nodes);
    drop(assignments);

    let messages: Vec<psyche_inference::ChatMessage> = req
        .messages
        .iter()
        .map(|m| psyche_inference::ChatMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        })
        .collect();

    let request_id = uuid::Uuid::new_v4().to_string();
    let inference_req = InferenceRequest {
        request_id: request_id.clone(),
        messages,
        max_tokens: req.max_tokens.unwrap_or(100),
        temperature: req.temperature.unwrap_or(1.0),
        top_p: req.top_p.unwrap_or(1.0),
        stream: req.stream,
    };

    let (tx, mut rx) = mpsc::channel(1);

    state
        .pending_requests
        .write()
        .await
        .insert(request_id.clone(), tx);

    let msg = InferenceMessage::Request(inference_req);
    if let Err(e) = state.network_tx.send((target_peer_id, msg)).await {
        error!("Failed to send inference request: {:#}", e);
        state.pending_requests.write().await.remove(&request_id);
        return Err(AppError::InternalError);
    }

    info!("Sent inference request {} to network", request_id);

    let response = tokio::time::timeout(Duration::from_secs(30), rx.recv())
        .await
        .map_err(|_| AppError::Timeout)?
        .ok_or(AppError::InternalError)?;

    state.pending_requests.write().await.remove(&request_id);

    let created = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Ok(Json(ChatCompletionResponse {
        id: format!("chatcmpl-{}", response.request_id),
        object: "chat.completion".to_string(),
        created,
        model: model_name,
        choices: vec![ChatCompletionChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: response.generated_text,
            },
            finish_reason: response.finish_reason,
        }],
    }))
}

#[axum::debug_handler]
async fn handle_assign_models(
    State(state): State<Arc<GatewayState>>,
    Json(req): Json<AssignModelsRequest>,
) -> Result<String, AppError> {
    use psyche_inference::ModelSource;

    info!(
        "Admin API: Received assign-models request with {} specs",
        req.assignments.len()
    );

    let mut assigned_count = 0;
    let mut total_requested = 0;

    for spec in req.assignments {
        total_requested += spec.num_nodes;

        info!(
            "Assigning {} nodes to model: {}",
            spec.num_nodes, spec.model_name
        );

        // Get available nodes
        let nodes = state.available_nodes.read().await;
        let assignments = state.model_assignments.read().await;

        // Find idle nodes (not currently assigned)
        let idle_nodes: Vec<EndpointId> = nodes
            .keys()
            .filter(|node_id| !assignments.contains_key(*node_id))
            .copied()
            .take(spec.num_nodes)
            .collect();

        if idle_nodes.len() < spec.num_nodes {
            warn!(
                "Only {} idle nodes available, requested {}",
                idle_nodes.len(),
                spec.num_nodes
            );
        }

        drop(nodes);
        drop(assignments);

        // Build model source
        let model_source = match spec.source_type {
            ModelSourceType::HuggingFace => {
                let path = spec.source_path.unwrap_or_else(|| spec.model_name.clone());
                ModelSource::HuggingFace(path)
            }
            ModelSourceType::Local => {
                let path = spec.source_path.ok_or_else(|| {
                    AppError::BadRequest("source_path is required for local models".to_string())
                })?;
                ModelSource::Local(path)
            }
        };

        // Assign and send LoadModel to each selected node
        for node_id in idle_nodes {
            // Update assignments map
            state
                .model_assignments
                .write()
                .await
                .insert(node_id, spec.model_name.clone());

            // Broadcast LoadModel to the specific node
            let load_msg = InferenceGossipMessage::LoadModel {
                model_name: spec.model_name.clone(),
                model_source: model_source.clone(),
            };

            if let Err(e) = state.gossip_tx.send(load_msg).await {
                error!(
                    "Failed to send LoadModel to node {}: {:#}",
                    node_id.fmt_short(),
                    e
                );
            } else {
                info!(
                    "Sent LoadModel to node {} for model {}",
                    node_id.fmt_short(),
                    spec.model_name
                );
                assigned_count += 1;
            }
        }
    }

    // Persist assignments to disk
    let assignments = state.model_assignments.read().await;
    if let Err(e) = save_assignments(ASSIGNMENTS_FILE, &assignments) {
        error!("Failed to save assignments: {:#}", e);
    }
    drop(assignments);

    info!(
        "Assignment complete: {} nodes assigned out of {} requested",
        assigned_count, total_requested
    );

    Ok(format!(
        "Assigned {} nodes out of {} requested",
        assigned_count, total_requested
    ))
}

#[axum::debug_handler]
async fn handle_get_assignments(
    State(state): State<Arc<GatewayState>>,
) -> Json<Vec<AssignmentInfo>> {
    let assignments = state.model_assignments.read().await;
    let nodes = state.available_nodes.read().await;

    let mut result = Vec::new();

    for (node_id, assigned_model) in assignments.iter() {
        let status = match nodes.get(node_id) {
            None => {
                info!(
                    "Node {} not in available_nodes (offline)",
                    node_id.fmt_short()
                );
                "offline".to_string()
            }
            Some(node_info) => match &node_info.model_name {
                None => {
                    info!(
                        "Node {} has no model loaded (assigned: {})",
                        node_id.fmt_short(),
                        assigned_model
                    );
                    "idle".to_string()
                }
                Some(current_model) if current_model == assigned_model => {
                    info!(
                        "Node {} loaded correct model: {}",
                        node_id.fmt_short(),
                        current_model
                    );
                    "loaded".to_string()
                }
                Some(current_model) => {
                    info!(
                        "Node {} has model '{}' but assigned model is '{}'",
                        node_id.fmt_short(),
                        current_model,
                        assigned_model
                    );
                    "loading".to_string() // Has different model, probably loading
                }
            },
        };

        result.push(AssignmentInfo {
            node_id: node_id.to_string(),
            model_name: assigned_model.clone(),
            status,
        });
    }

    Json(result)
}

#[derive(Debug)]
enum AppError {
    NoNodesAvailable,
    Timeout,
    InternalError,
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NoNodesAvailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "No inference nodes available".to_string(),
            ),
            AppError::Timeout => (
                StatusCode::GATEWAY_TIMEOUT,
                "Inference request timed out".to_string(),
            ),
            AppError::InternalError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            ),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
        };
        (status, message).into_response()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    run_gateway().await
}

async fn send_inference_request(
    endpoint: iroh::Endpoint,
    peer_id: EndpointId,
    request: InferenceRequest,
) -> Result<InferenceResponse> {
    info!(
        "Connecting to peer {} with ALPN {:?}",
        peer_id.fmt_short(),
        std::str::from_utf8(INFERENCE_ALPN)
    );

    // connect to peer and open bidirectional stream
    let connection = endpoint
        .connect(peer_id, INFERENCE_ALPN)
        .await
        .context("Failed to connect to peer")?;

    info!("Connected, opening bidirectional stream");
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .context("Failed to open bidirectional stream")?;

    let message = InferenceMessage::Request(request);
    let request_bytes =
        postcard::to_allocvec(&message).context("Failed to serialize inference request")?;

    info!("Sending {} bytes", request_bytes.len());
    send.write_all(&request_bytes)
        .await
        .context("Failed to write request")?;

    info!("Finishing send stream");
    send.finish()?;

    info!("Reading response...");
    let response_bytes = recv
        .read_to_end(10 * 1024 * 1024)
        .await
        .context("Failed to read response")?; // 10MB max

    info!("Received {} bytes, deserializing", response_bytes.len());
    let response_message: InferenceMessage = postcard::from_bytes(&response_bytes)
        .context("Failed to deserialize inference response")?;

    match response_message {
        InferenceMessage::Response(response) => {
            info!("Successfully received inference response");
            Ok(response)
        }
        _ => anyhow::bail!("Unexpected message type from inference node"),
    }
}

async fn run_gateway() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    info!("Starting gateway node");
    info!("  HTTP API: http://{}", args.listen_addr);
    info!("  Discovery mode: {:?}", args.discovery_mode);
    info!("  Relay kind: {:?}", args.relay_kind);

    let bootstrap_peers = psyche_inference_node::load_bootstrap_peers(
        args.bootstrap_peer_file.as_ref(),
        "No bootstrap peers configured (gateway will be a bootstrap node)",
    )?;

    let cancel = CancellationToken::new();

    info!("Initializing P2P network...");
    let metrics = Arc::new(ClientMetrics::default());
    let run_id = "inference";

    type P2PNetwork = NetworkConnection<InferenceGossipMessage, ()>;

    let mut network = P2PNetwork::init(
        run_id,
        None,
        None,
        args.discovery_mode,
        args.relay_kind,
        bootstrap_peers,
        None,
        allowlist::AllowAll,
        metrics.clone(),
        Some(cancel.clone()),
    )
    .await
    .context("Failed to initialize P2P network")?;

    info!("P2P network initialized");
    info!("  Endpoint ID: {}", network.endpoint_id());

    // write endpoint to file if requested
    let endpoint_file = if let Ok(file_path) = std::env::var("PSYCHE_GATEWAY_ENDPOINT_FILE") {
        info!("Found PSYCHE_GATEWAY_ENDPOINT_FILE env var: {}", file_path);
        Some(PathBuf::from(file_path))
    } else {
        info!("No PSYCHE_GATEWAY_ENDPOINT_FILE env var, checking CLI args");
        args.write_endpoint_file.clone()
    };

    let endpoint_addr = network.router().endpoint().addr();
    let endpoints = vec![endpoint_addr];

    if let Some(ref endpoint_file) = endpoint_file {
        let content =
            serde_json::to_string(&endpoints).context("Failed to serialize endpoint address")?;
        fs::write(endpoint_file, content).context("Failed to write endpoint file")?;
        info!("Wrote gateway endpoint to {:?}", endpoint_file);
        info!("Other nodes can bootstrap using this file");
    } else {
        let endpoint_json = serde_json::to_string_pretty(&endpoints)
            .context("Failed to serialize endpoint address")?;
        info!("Gateway endpoint address (use for bootstrapping inference nodes):");
        println!("\n{}\n", endpoint_json);
    }

    info!("Waiting for gossip mesh to stabilize...");
    sleep(Duration::from_secs(5)).await;

    info!("Gossip mesh should be ready");

    let (network_tx, mut network_rx) = mpsc::channel::<(EndpointId, InferenceMessage)>(100);
    let (gossip_tx, mut gossip_rx) = mpsc::channel::<InferenceGossipMessage>(100);

    // Load persisted model assignments
    let model_assignments = load_assignments(ASSIGNMENTS_FILE);

    let state = Arc::new(GatewayState {
        available_nodes: RwLock::new(HashMap::new()),
        pending_requests: RwLock::new(HashMap::new()),
        model_assignments: RwLock::new(model_assignments),
        network_tx,
        gossip_tx,
    });

    info!("Gateway ready! Listening on http://{}", args.listen_addr);
    info!("Discovering inference nodes...");

    let network_handle = {
        let state = state.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let mut task_set = tokio::task::JoinSet::new();

            // Reconciliation timer - check for assignment drift every 60s
            let mut reconciliation_interval = tokio::time::interval(Duration::from_secs(60));
            reconciliation_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("Network task shutting down");
                        info!("Aborting {} active P2P request tasks", task_set.len());
                        task_set.shutdown().await;
                        break;
                    }

                    _ = reconciliation_interval.tick() => {
                        // Check for drift: nodes that are assigned but not serving the right model
                        use psyche_inference::ModelSource;

                        let assignments = state.model_assignments.read().await;
                        let nodes = state.available_nodes.read().await;

                        for (node_id, assigned_model) in assignments.iter() {
                            match nodes.get(node_id) {
                                None => {
                                    // Node offline, nothing to do - wait for it to come back
                                    debug!("Node {} offline (assigned: {})", node_id.fmt_short(), assigned_model);
                                }
                                Some(node_info) => {
                                    let needs_reload = match &node_info.model_name {
                                        None => {
                                            // Node is idle but should have a model
                                            warn!("Node {} is idle, should be serving: {}", node_id.fmt_short(), assigned_model);
                                            true
                                        }
                                        Some(current_model) if current_model != assigned_model => {
                                            // Node has wrong model
                                            warn!("Node {} has {} but should have {}",
                                                  node_id.fmt_short(), current_model, assigned_model);
                                            true
                                        }
                                        _ => false, // All good
                                    };

                                    if needs_reload {
                                        info!("Sending LoadModel to node {} for model {}",
                                              node_id.fmt_short(), assigned_model);

                                        // Re-send LoadModel (assuming HuggingFace for now)
                                        // TODO: store source_type with assignment
                                        let load_msg = InferenceGossipMessage::LoadModel {
                                            model_name: assigned_model.clone(),
                                            model_source: ModelSource::HuggingFace(assigned_model.clone()),
                                        };

                                        if let Err(e) = network.broadcast(&load_msg) {
                                            error!("Failed to broadcast LoadModel for reconciliation: {:#}", e);
                                        }
                                    }
                                }
                            }
                        }

                        drop(assignments);
                        drop(nodes);
                    }

                    Some((target_peer_id, msg)) = network_rx.recv() => {
                        match msg {
                            InferenceMessage::Request(req) => {
                                let request_id = req.request_id.clone();
                                info!("Sending inference request {} to {} via direct P2P",
                                      request_id, target_peer_id.fmt_short());

                                let endpoint = network.router().endpoint().clone();
                                let state_clone = state.clone();
                                task_set.spawn(async move {
                                    // timeout slightly longer than HTTP handler timeout (30s) to avoid race - might need to adjust
                                    let result = tokio::time::timeout(
                                        Duration::from_secs(35),
                                        send_inference_request(endpoint, target_peer_id, req)
                                    ).await;

                                    match result {
                                        Ok(Ok(response)) => {
                                            info!("Received inference response for {}", request_id);
                                            if let Some(tx) = state_clone.pending_requests.write().await.remove(&request_id) {
                                                let _ = tx.send(response).await;
                                            }
                                        }
                                        Ok(Err(e)) => {
                                            error!("Failed to send inference request: {:#}", e);
                                            state_clone.pending_requests.write().await.remove(&request_id);
                                        }
                                        Err(_) => {
                                            error!("Inference request {} timed out after 35s", request_id);
                                            state_clone.pending_requests.write().await.remove(&request_id);
                                        }
                                    }
                                });
                            }
                            _ => continue,
                        };
                    }

                    Some(_) = task_set.join_next(), if !task_set.is_empty() => {
                    }

                    Some(gossip_msg) = gossip_rx.recv() => {
                        info!("Broadcasting gossip message: {:?}", gossip_msg);
                        if let Err(e) = network.broadcast(&gossip_msg) {
                            error!("Failed to broadcast gossip message: {:#}", e);
                        } else {
                            info!("Successfully broadcasted gossip message");
                        }
                    }

                    Some(gossip_msg) = gossip_rx.recv() => {
                        info!("Broadcasting gossip message: {:?}", gossip_msg);
                        if let Err(e) = network.broadcast(&gossip_msg) {
                            error!("Failed to broadcast gossip message: {:#}", e);
                        } else {
                            info!("Successfully broadcasted gossip message");
                        }
                    }

                    event = network.poll_next() => {
                        match event {
                            Ok(Some(NetworkEvent::MessageReceived((peer_id, msg)))) => {
                                info!("Received gossip message from {}", peer_id.fmt_short());
                                match msg {
                                    InferenceGossipMessage::NodeAvailable { model_name, checkpoint_id, capabilities } => {
                                        info!("Discovered inference node!");
                                        info!("  Peer ID: {}", peer_id.fmt_short());
                                        info!("  Model: {}", model_name.as_deref().unwrap_or("<idle>"));
                                        info!("  Checkpoint: {:?}", checkpoint_id);
                                        info!("  Capabilities: {:?}", capabilities);

                                        let node_info = InferenceNodeInfo {
                                            peer_id,
                                            model_name,
                                            checkpoint_id,
                                            capabilities,
                                        };
                                        state.available_nodes.write().await.insert(peer_id, node_info);
                                    }
                                    InferenceGossipMessage::NodeUnavailable => {
                                        info!("Inference node {} went offline", peer_id.fmt_short());
                                        state.available_nodes.write().await.remove(&peer_id);
                                    }
                                    InferenceGossipMessage::LoadModel { .. } => {
                                        debug!("Ignoring LoadModel message (gateways don't load models)");
                                    }
                                    InferenceGossipMessage::ReloadCheckpoint { checkpoint_id, checkpoint_source } => {
                                        debug!("Checkpoint reload notification: {} from {}", checkpoint_id, checkpoint_source);
                                    }
                                }
                            }
                            Ok(Some(_)) => {
                                debug!("Other network event (ignored)");
                            }
                            Ok(None) => {}
                            Err(e) => {
                                error!("Network error: {:#}", e);
                            }
                        }
                    }
                }
            }
        })
    };

    let app = Router::new()
        .route("/v1/chat/completions", post(handle_inference))
        .route("/admin/assign-models", post(handle_assign_models))
        .route(
            "/admin/assignments",
            axum::routing::get(handle_get_assignments),
        )
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&args.listen_addr)
        .await
        .context("Failed to bind HTTP server")?;

    info!("HTTP server listening on {}", args.listen_addr);

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .context("HTTP server error")
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = cancel.cancelled() => {
            info!("Cancellation requested");
        }
    }

    info!("Shutting down...");
    cancel.cancel();

    let _ = tokio::join!(network_handle, server_handle);

    info!("Shutdown complete");
    Ok(())
}
