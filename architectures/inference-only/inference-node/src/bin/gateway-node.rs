//! Gateway node for inference requests
//!
//! Usage:
//! 1. Exposes HTTP API on localhost:8000
//! 2. Discovers inference nodes via gossip
//! 3. Routes requests to available inference nodes via gossip
//! 4. Returns responses to HTTP clients
//!
//!   cargo run --bin gateway-node --features gateway -- --discovery-mode local

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[cfg(feature = "gateway")]
use anyhow::Context;
#[cfg(feature = "gateway")]
use iroh::EndpointAddr;
#[cfg(feature = "gateway")]
use psyche_inference::{
    INFERENCE_ALPN, InferenceGossipMessage, InferenceMessage, InferenceRequest, InferenceResponse,
};
#[cfg(feature = "gateway")]
use psyche_metrics::ClientMetrics;
#[cfg(feature = "gateway")]
use psyche_network::{
    DiscoveryMode, EndpointId, NetworkConnection, NetworkEvent, RelayKind, allowlist,
};
#[cfg(feature = "gateway")]
use std::{collections::HashMap, fs, sync::Arc, time::Duration};
#[cfg(feature = "gateway")]
use tokio::{
    sync::{RwLock, mpsc},
    time::sleep,
};
#[cfg(feature = "gateway")]
use tokio_util::sync::CancellationToken;
#[cfg(feature = "gateway")]
use tracing::{debug, error, info};

#[cfg(feature = "gateway")]
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8000")]
    listen_addr: String,

    #[arg(long, default_value = "local")]
    discovery_mode: String,

    #[arg(long, default_value = "disabled")]
    relay_kind: String,

    #[arg(long)]
    bootstrap_peer_file: Option<PathBuf>,

    #[arg(long)]
    write_endpoint_file: Option<PathBuf>,
}

#[cfg(feature = "gateway")]
#[derive(Clone, Debug)]
struct InferenceNodeInfo {
    peer_id: EndpointId,
    model_name: String,
    #[allow(dead_code)]
    checkpoint_id: Option<String>,
    #[allow(dead_code)]
    capabilities: Vec<String>,
}

#[cfg(feature = "gateway")]
struct GatewayState {
    available_nodes: RwLock<HashMap<EndpointId, InferenceNodeInfo>>,
    pending_requests: RwLock<HashMap<String, mpsc::Sender<InferenceResponse>>>,
    network_tx: mpsc::Sender<InferenceMessage>,
}

#[cfg(feature = "gateway")]
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
struct ChatMessage {
    role: String,
    content: String,
}

#[cfg(feature = "gateway")]
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

#[cfg(feature = "gateway")]
fn default_max_tokens() -> Option<usize> {
    Some(100)
}
#[cfg(feature = "gateway")]
fn default_temperature() -> Option<f64> {
    Some(1.0)
}
#[cfg(feature = "gateway")]
fn default_top_p() -> Option<f64> {
    Some(1.0)
}

#[cfg(feature = "gateway")]
#[derive(serde::Serialize)]
struct ChatCompletionChoice {
    index: usize,
    message: ChatMessage,
    finish_reason: Option<String>,
}

#[cfg(feature = "gateway")]
#[derive(serde::Serialize)]
struct ChatCompletionResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<ChatCompletionChoice>,
    // we're omitting usage stats for now
}

#[cfg(feature = "gateway")]
#[axum::debug_handler]
async fn handle_inference(
    State(state): State<Arc<GatewayState>>,
    Json(req): Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, AppError> {
    let nodes = state.available_nodes.read().await;
    if nodes.is_empty() {
        return Err(AppError::NoNodesAvailable);
    }

    let node = nodes.values().next().unwrap();
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
    if let Err(e) = state.network_tx.send(msg).await {
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

#[cfg(feature = "gateway")]
#[derive(Debug)]
enum AppError {
    NoNodesAvailable,
    Timeout,
    InternalError,
}

#[cfg(feature = "gateway")]
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
    #[cfg(not(feature = "gateway"))]
    {
        eprintln!("Gateway node requires the 'gateway' feature to be enabled.");
        eprintln!("Build with: cargo run --bin gateway-node --features gateway");
        std::process::exit(1);
    }

    #[cfg(feature = "gateway")]
    run_gateway().await
}

#[cfg(feature = "gateway")]
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

#[cfg(feature = "gateway")]
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

    let discovery_mode: DiscoveryMode = args
        .discovery_mode
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid discovery mode: {}", e))?;

    let relay_kind: RelayKind = args
        .relay_kind
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid relay kind: {}", e))?;

    // read bootstrap peers from multiple sources in priority order
    let bootstrap_peers: Vec<EndpointAddr> =
        if let Ok(endpoints_json) = std::env::var("PSYCHE_GATEWAY_ENDPOINTS") {
            // JSON array of other gateway endpoints
            info!("Reading gateway endpoints from PSYCHE_GATEWAY_ENDPOINTS env var");
            let peers: Vec<EndpointAddr> = serde_json::from_str(&endpoints_json)
                .context("Failed to parse PSYCHE_GATEWAY_ENDPOINTS as JSON array")?;
            info!("Loaded {} gateway endpoint(s) from env var", peers.len());
            for peer in &peers {
                info!("  Gateway: {}", peer.id.fmt_short());
            }
            peers
        } else if let Ok(file_path) = std::env::var("PSYCHE_GATEWAY_BOOTSTRAP_FILE") {
            // env var pointing to file
            let peer_file = PathBuf::from(file_path);
            if peer_file.exists() {
                info!(
                    "Reading bootstrap peers from PSYCHE_GATEWAY_BOOTSTRAP_FILE: {:?}",
                    peer_file
                );
                let content = fs::read_to_string(&peer_file)
                    .context("Failed to read gateway bootstrap file")?;
                let peers: Vec<EndpointAddr> = serde_json::from_str(&content)
                    .context("Failed to parse gateway bootstrap file as JSON array")?;
                info!("Loaded {} gateway endpoint(s) from file", peers.len());
                peers
            } else {
                info!("Gateway bootstrap file not found, starting without peers");
                vec![]
            }
        } else if let Some(ref peer_file) = args.bootstrap_peer_file {
            // local testing: CLI argument
            if peer_file.exists() {
                info!("Reading bootstrap peer from {:?}", peer_file);
                let content =
                    fs::read_to_string(peer_file).context("Failed to read bootstrap peer file")?;
                // support both single endpoint and array
                if let Ok(peer) = serde_json::from_str::<EndpointAddr>(&content) {
                    info!("Bootstrap peer: {}", peer.id.fmt_short());
                    vec![peer]
                } else {
                    let peers: Vec<EndpointAddr> = serde_json::from_str(&content)
                        .context("Failed to parse bootstrap peer file")?;
                    info!("Loaded {} bootstrap peer(s)", peers.len());
                    peers
                }
            } else {
                info!("Bootstrap peer file not found, starting without peers");
                vec![]
            }
        } else {
            info!("No bootstrap peers configured (gateway will be a bootstrap node)");
            vec![]
        };

    let cancel = CancellationToken::new();

    info!("Initializing P2P network...");
    let metrics = Arc::new(ClientMetrics::default());
    let run_id = "inference";

    type P2PNetwork = NetworkConnection<InferenceGossipMessage, ()>;

    let mut network = P2PNetwork::init(
        run_id,
        None,
        None,
        discovery_mode,
        relay_kind,
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

    let (network_tx, mut network_rx) = mpsc::channel::<InferenceMessage>(100);
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
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("Network task shutting down");
                        break;
                    }

                    Some(msg) = network_rx.recv() => {
                        match msg {
                            InferenceMessage::Request(req) => {
                                // Send request via direct P2P connection
                                let request_id = req.request_id.clone();
                                info!("Sending inference request {} via direct P2P", request_id);

                                // Get target node
                                let nodes = state.available_nodes.read().await;
                                let target_node = match nodes.values().next() {
                                    Some(node) => node.peer_id,
                                    None => {
                                        error!("No inference nodes available");
                                        continue;
                                    }
                                };
                                drop(nodes);

                                // Spawn task to handle P2P connection
                                let endpoint = network.router().endpoint().clone();
                                let state_clone = state.clone();
                                tokio::spawn(async move {
                                    match send_inference_request(endpoint, target_node, req).await {
                                        Ok(response) => {
                                            info!("Received inference response for {}", request_id);
                                            // Forward response to pending request
                                            if let Some(tx) = state_clone.pending_requests.write().await.remove(&request_id) {
                                                let _ = tx.send(response).await;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Failed to send inference request: {:#}", e);
                                        }
                                    }
                                });
                            }
                            _ => continue,
                        };
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
