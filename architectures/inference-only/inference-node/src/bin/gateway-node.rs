//! Gateway node for inference requests
//!
//! Usage:
//! 1. Exposes HTTP API on localhost:8000
//! 2. Discovers inference nodes via gossip
//! 3. Routes requests to available inference nodes via gossip
//! 4. Returns responses to HTTP clients
//!
//!   cargo run --bin gateway-node --features gateway -- --discovery-mode local

use anyhow::{Context, Result};
use clap::Parser;
use iroh::{EndpointAddr, protocol::Router};
use psyche_inference::{
    INFERENCE_ALPN, InferenceGossipMessage, InferenceMessage, InferenceRequest, InferenceResponse,
};
use psyche_metrics::ClientMetrics;
use psyche_network::{
    DiscoveryMode, EndpointId, NetworkConnection, NetworkEvent, RelayKind, allowlist,
};
use std::{collections::HashMap, fs, path::PathBuf, sync::Arc, time::Duration};
use tokio::{sync::RwLock, time::sleep};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

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
}

#[derive(Clone, Debug)]
struct InferenceNodeInfo {
    peer_id: EndpointId,
    model_name: String,
    checkpoint_id: Option<String>,
    capabilities: Vec<String>,
}

struct GatewayState {
    available_nodes: RwLock<HashMap<EndpointId, InferenceNodeInfo>>,
    router: Arc<Router>,
}

#[cfg(feature = "gateway")]
#[derive(serde::Deserialize)]
struct InferenceRequestBody {
    prompt: String,
    #[serde(default = "default_max_tokens")]
    max_tokens: usize,
    #[serde(default = "default_temperature")]
    temperature: f64,
    #[serde(default = "default_top_p")]
    top_p: f64,
}

fn default_max_tokens() -> usize {
    100
}
fn default_temperature() -> f64 {
    1.0
}
fn default_top_p() -> f64 {
    1.0
}

#[cfg(feature = "gateway")]
#[derive(serde::Serialize)]
struct InferenceResponseBody {
    request_id: String,
    generated_text: String,
    full_text: String,
    finish_reason: Option<String>,
}

#[cfg(feature = "gateway")]
async fn handle_inference(
    State(state): State<Arc<GatewayState>>,
    Json(req): Json<InferenceRequestBody>,
) -> Result<Json<InferenceResponseBody>, AppError> {
    let nodes = state.available_nodes.read().await;
    if nodes.is_empty() {
        return Err(AppError::NoNodesAvailable);
    }

    let node = nodes.values().next().unwrap().clone();
    drop(nodes);

    info!(
        "Routing request to node: {} (model: {})",
        node.peer_id.fmt_short(),
        node.model_name
    );

    let request_id = uuid::Uuid::new_v4().to_string();
    let inference_req = InferenceRequest {
        request_id: request_id.clone(),
        prompt: req.prompt,
        max_tokens: req.max_tokens,
        temperature: req.temperature,
        top_p: req.top_p,
        stream: false,
    };

    let response = send_inference_request(state.router.clone(), node.peer_id, inference_req)
        .await
        .map_err(|e| {
            error!("Inference request failed: {:#}", e);
            AppError::InternalError
        })?;

    Ok(Json(InferenceResponseBody {
        request_id: response.request_id,
        generated_text: response.generated_text,
        full_text: response.full_text,
        finish_reason: response.finish_reason,
    }))
}

#[cfg(feature = "gateway")]
async fn send_inference_request(
    router: Arc<Router>,
    peer_id: EndpointId,
    request: InferenceRequest,
) -> Result<InferenceResponse> {
    info!("Connecting to inference node {}", peer_id.fmt_short());

    let conn = router
        .endpoint()
        .connect(peer_id, INFERENCE_ALPN)
        .await
        .context("Failed to connect to inference node")?;

    let (mut send, mut recv) = conn.open_bi().await.context("Failed to open stream")?;

    let request_msg = InferenceMessage::Request(request);
    let request_bytes =
        postcard::to_allocvec(&request_msg).context("Failed to serialize request")?;

    send.write_all(&request_bytes)
        .await
        .context("Failed to send request")?;
    send.finish().context("Failed to finish send")?;

    info!("Request sent, waiting for response...");

    let response_bytes = recv
        .read_to_end(1024 * 1024)
        .await
        .context("Failed to read response")?; // 1MB max

    let response_msg: InferenceMessage =
        postcard::from_bytes(&response_bytes).context("Failed to deserialize response")?;

    match response_msg {
        InferenceMessage::Response(response) => {
            info!("Received response for request {}", response.request_id);
            Ok(response)
        }
        _ => Err(anyhow::anyhow!("Unexpected response message type")),
    }
}

#[cfg(feature = "gateway")]
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

    let bootstrap_peers = if let Some(ref peer_file) = args.bootstrap_peer_file {
        if peer_file.exists() {
            info!("Reading bootstrap peer from {:?}", peer_file);
            let content =
                fs::read_to_string(peer_file).context("Failed to read bootstrap peer file")?;
            let endpoint_addr: EndpointAddr = serde_json::from_str(&content)
                .context("Failed to parse bootstrap peer endpoint")?;
            info!("Bootstrap peer: {}", endpoint_addr.id.fmt_short());
            vec![endpoint_addr]
        } else {
            info!("Bootstrap peer file not found, starting without peers");
            vec![]
        }
    } else {
        info!("No bootstrap peer file specified");
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

    info!("Waiting for gossip mesh to stabilize...");
    sleep(Duration::from_secs(2)).await;

    let state = Arc::new(GatewayState {
        available_nodes: RwLock::new(HashMap::new()),
        router: network.router(),
    });

    info!("Gateway ready! Listening on http://{}", args.listen_addr);
    info!("Discovering inference nodes...");

    // P2P network events
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

                    event = network.poll_next() => {
                        match event {
                            Ok(Some(NetworkEvent::MessageReceived((peer_id, msg)))) => {
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
        .route("/v1/inference", post(handle_inference))
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
