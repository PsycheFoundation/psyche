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
use clap::{Args as ClapArgs, Parser, Subcommand};
use psyche_inference::{InferenceGossipMessage, InferenceNode};
use psyche_metrics::ClientMetrics;
use psyche_network::{DiscoveryMode, NetworkConnection, NetworkEvent, RelayKind, allowlist};
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "psyche-inference-node")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    run_args: RunArgs,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the inference node (default)
    Run(RunArgs),

    // Prints the help, optionally as markdown. Used for docs generation.
    #[clap(hide = true)]
    PrintAllHelp {
        #[arg(long, required = true)]
        markdown: bool,
    },
}

#[derive(ClapArgs, Debug, Clone)]
struct RunArgs {
    #[arg(long)]
    model_name: Option<String>,

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

    let cli = Cli::parse();

    // If no subcommand is provided, default to run with the flattened args
    let run_args = match cli.command {
        Some(Commands::PrintAllHelp { markdown }) => {
            assert!(markdown);
            clap_markdown::print_help_markdown::<Cli>();
            return Ok(());
        }
        Some(Commands::Run(args)) => args,
        None => cli.run_args,
    };

    let model_name = run_args.model_name.context("--model-name is required")?;

    info!("Starting Psyche Inference Node");
    info!("Model: {}", model_name);
    info!("Tensor Parallel Size: {}", run_args.tensor_parallel_size);
    info!(
        "GPU Memory Utilization: {}",
        run_args.gpu_memory_utilization
    );

    let discovery_mode: DiscoveryMode = run_args
        .discovery_mode
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid discovery mode: {}", e))?;

    let relay_kind: RelayKind = run_args
        .relay_kind
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid relay kind: {}", e))?;

    let capabilities: Vec<String> = if run_args.capabilities.is_empty() {
        vec![]
    } else {
        run_args
            .capabilities
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
        model_name.clone(),
        Some(run_args.tensor_parallel_size),
        Some(run_args.gpu_memory_utilization),
    );

    inference_node
        .initialize(
            Some(run_args.tensor_parallel_size),
            Some(run_args.gpu_memory_utilization),
        )
        .context("Failed to initialize vLLM engine")?;

    info!("vLLM engine initialized successfully");

    info!("Initializing P2P network...");

    let metrics = Arc::new(ClientMetrics::default());
    let run_id = "inference";

    type P2PNetwork = NetworkConnection<InferenceGossipMessage, ()>;

    let mut network = P2PNetwork::init(
        run_id,
        None, // port (let OS choose)
        None, // interface
        discovery_mode,
        relay_kind,
        vec![],              // bootstrap peers (will discover via gossip)
        None,                // secret key (generate new)
        allowlist::AllowAll, // No allowlist for inference network
        metrics.clone(),
        Some(cancel.clone()),
    )
    .await
    .context("Failed to initialize P2P network")?;

    info!("✓ P2P network initialized");
    info!("  Endpoint ID: {}", network.endpoint_id());

    // Announce availability via gossip
    let availability_msg = InferenceGossipMessage::NodeAvailable {
        model_name: model_name.clone(),
        checkpoint_id: None, // TODO: Track actual checkpoint when reloading - do we need this?
        capabilities: capabilities.clone(),
    };

    network
        .broadcast(&availability_msg)
        .context("Failed to broadcast availability")?;

    info!("Broadcasted availability to network");
    info!("Inference node ready! Listening for requests...");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal");
                break;
            }

            _ = cancel.cancelled() => {
                info!("Cancellation requested");
                break;
            }

            event = network.poll_next() => {
                match event {
                    Ok(Some(NetworkEvent::MessageReceived((peer_id, msg)))) => {
                        debug!("Received gossip message from {}: {:?}", peer_id.fmt_short(), msg);

                        match msg {
                            InferenceGossipMessage::NodeAvailable { model_name, checkpoint_id, capabilities } => {
                                info!("Peer {} is available: model={}, checkpoint={:?}, caps={:?}",
                                      peer_id.fmt_short(), model_name, checkpoint_id, capabilities);
                            }
                            InferenceGossipMessage::NodeUnavailable => {
                                info!("Peer {} is no longer available", peer_id.fmt_short());
                            }
                            InferenceGossipMessage::ReloadCheckpoint { checkpoint_id, checkpoint_source } => {
                                info!("Received checkpoint reload request: {} from {}",
                                      checkpoint_id, checkpoint_source);
                                // TODO: Implement checkpoint reloading - used for changing mdoels? need to figure this out
                                warn!("Checkpoint reloading not yet implemented");
                            }
                        }
                    }
                    Ok(Some(NetworkEvent::DownloadComplete(_))) => {
                        // not used for now
                        debug!("Download complete event");
                    }
                    Ok(Some(NetworkEvent::DownloadFailed(_))) => {
                        warn!("Download failed event");
                    }
                    Ok(Some(NetworkEvent::ParameterRequest(..))) |
                    Ok(Some(NetworkEvent::ModelConfigRequest(..))) => {
                        // not used for inference nodes
                        debug!("Parameter/config request (ignored)");
                    }
                    Ok(None) => {
                    }
                    Err(e) => {
                        error!("Network error: {:#}", e);
                    }
                }
            }
        }
    }

    info!("Shutting down inference node...");
    inference_node.shutdown()?;
    info!("Shutdown complete");

    Ok(())
}
