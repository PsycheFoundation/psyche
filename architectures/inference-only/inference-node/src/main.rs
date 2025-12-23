//! Psyche Inference Node
//!
//! A standalone node for serving LLM inference over the Psyche P2P network.
//!
//! Architecture:
//! - Joins P2P network via iroh (gossip + direct connections)
//! - Announces availability via gossip
//! - Handles inference requests via direct P2P connections
//! - Supports dynamic checkpoint reloading

use anyhow::{Context, Result};
use clap::Parser;
use psyche_inference::{InferenceGossipMessage, InferenceNode};
use psyche_network::{DiscoveryMode, RelayKind};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "psyche-inference-node")]
struct Args {
    #[arg(long)]
    model_name: String,

    #[arg(long, default_value = "1")]
    tensor_parallel_size: usize,

    #[arg(long, default_value = "0.9")]
    gpu_memory_utilization: f64,

    #[arg(long)]
    checkpoint_path: Option<PathBuf>,

    #[arg(long, default_value = "n0")]
    discovery_mode: String,

    #[arg(long, default_value = "n0")]
    relay_kind: String,

    #[arg(long)]
    relay_url: Option<String>,

    /// node capabilities (comma-separated, e.g. "streaming,tool_use")
    #[arg(long, default_value = "")]
    capabilities: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    info!("Starting Psyche Inference Node");
    info!("Model: {}", args.model_name);
    info!("Tensor Parallel Size: {}", args.tensor_parallel_size);
    info!("GPU Memory Utilization: {}", args.gpu_memory_utilization);

    let discovery_mode: DiscoveryMode = args
        .discovery_mode
        .parse()
        .context("Invalid discovery mode")?;

    let relay_kind: RelayKind = args.relay_kind.parse().context("Invalid relay kind")?;

    let capabilities: Vec<String> = if args.capabilities.is_empty() {
        vec![]
    } else {
        args.capabilities
            .split(',')
            .map(|s| s.trim().to_string())
            .collect()
    };

    info!("Discovery mode: {:?}", discovery_mode);
    info!("Relay kind: {:?}", relay_kind);
    info!("Capabilities: {:?}", capabilities);

    let cancel = CancellationToken::new();

    info!("Initializing vLLM engine...");
    let mut inference_node = InferenceNode::new(
        args.model_name.clone(),
        Some(args.tensor_parallel_size),
        Some(args.gpu_memory_utilization),
    );

    inference_node
        .initialize(
            Some(args.tensor_parallel_size),
            Some(args.gpu_memory_utilization),
        )
        .context("Failed to initialize vLLM engine")?;

    info!("vLLM engine initialized successfully");

    // TODO: initialize P2P network
    info!("Initialize P2P network with iroh");

    // TODO: announce availability via gossip
    let availability_msg = InferenceGossipMessage::NodeAvailable {
        model_name: args.model_name.clone(),
        checkpoint_id: None, // TODO: Track actual checkpoint - is this needed?
        capabilities: capabilities.clone(),
    };
    info!("Broadcast availability: {:?}", availability_msg);

    // TODO: main event loop
    info!("Enter main event loop to handle inference requests");

    // for now, just wait for Ctrl+C
    info!("Inference node ready! Press Ctrl+C to shutdown.");

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = cancel.cancelled() => {
            info!("Cancellation requested");
        }
    }

    info!("Shutting down inference node...");
    inference_node.shutdown()?;
    info!("Shutdown complete");

    Ok(())
}
