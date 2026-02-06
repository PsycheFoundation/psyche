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
use tracing::{debug, error, info};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8000")]
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
    model_name: String,
    #[allow(dead_code)]
    checkpoint_id: Option<String>,
    #[allow(dead_code)]
    capabilities: Vec<String>,
}

struct GatewayState {
    available_nodes: RwLock<HashMap<EndpointId, InferenceNodeInfo>>,
    pending_requests: RwLock<HashMap<String, mpsc::Sender<InferenceResponse>>>,
    network_tx: mpsc::Sender<(EndpointId, InferenceMessage)>,
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
    let node = nodes.values().next().ok_or(AppError::NoNodesAvailable)?;

    let target_peer_id = node.peer_id;
    let model_name = req.model.clone().unwrap_or_else(|| node.model_name.clone());
    info!(
        "Routing request to node: {} (model: {})",
        node.peer_id.fmt_short(),
        node.model_name
    );
    drop(nodes);

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

#[derive(Debug)]
enum AppError {
    NoNodesAvailable,
    Timeout,
    InternalError,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NoNodesAvailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "No inference nodes available",
            ),
            AppError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "Inference request timed out"),
            AppError::InternalError => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
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

    if let Some(ref endpoint_file) = endpoint_file {
        let endpoint_addr = network.router().endpoint().addr();
        let endpoints = vec![endpoint_addr];
        let content =
            serde_json::to_string(&endpoints).context("Failed to serialize endpoint address")?;
        fs::write(endpoint_file, content).context("Failed to write endpoint file")?;
        info!("Wrote gateway endpoint to {:?}", endpoint_file);
        info!("Other nodes can bootstrap using this file");
    }

    info!("Waiting for gossip mesh to stabilize...");
    sleep(Duration::from_secs(5)).await;

    info!("Gossip mesh should be ready");

    let (network_tx, mut network_rx) = mpsc::channel::<(EndpointId, InferenceMessage)>(100);
    let state = Arc::new(GatewayState {
        available_nodes: RwLock::new(HashMap::new()),
        pending_requests: RwLock::new(HashMap::new()),
        network_tx,
    });

    info!("Gateway ready! Listening on http://{}", args.listen_addr);
    info!("Discovering inference nodes...");

    let network_handle = {
        let state = state.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let mut task_set = tokio::task::JoinSet::new();

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("Network task shutting down");
                        info!("Aborting {} active P2P request tasks", task_set.len());
                        task_set.shutdown().await;
                        break;
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

                    event = network.poll_next() => {
                        match event {
                            Ok(Some(NetworkEvent::MessageReceived((peer_id, msg)))) => {
                                info!("Received gossip message from {}", peer_id.fmt_short());
                                match msg {
                                    InferenceGossipMessage::NodeAvailable { model_name, checkpoint_id, capabilities } => {
                                        info!("Discovered inference node!");
                                        info!("  Peer ID: {}", peer_id.fmt_short());
                                        info!("  Model: {}", model_name);
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
