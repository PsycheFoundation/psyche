//! Test binary for P2P network without vLLM
//!
//! Usage:
//!   Terminal 1: cargo run --bin test-network -- --node-id 1
//!   Terminal 2: cargo run --bin test-network -- --node-id 2
//!
//! They should discover each other and see availability announcements.

use anyhow::{Context, Result};
use clap::Parser;
use psyche_inference::InferenceGossipMessage;
use psyche_metrics::ClientMetrics;
use psyche_network::{DiscoveryMode, NetworkConnection, NetworkEvent, RelayKind, allowlist};
use std::{fs, path::PathBuf, sync::Arc, time::Duration};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    node_id: String,

    #[arg(long, default_value = "local")]
    discovery_mode: String,

    #[arg(long, default_value = "disabled")]
    relay_kind: String,

    #[arg(long)]
    bootstrap_peer_file: Option<PathBuf>,

    #[arg(long)]
    write_endpoint_file: Option<PathBuf>,
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

    info!("Starting test node: {}", args.node_id);

    let discovery_mode: DiscoveryMode = args
        .discovery_mode
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid discovery mode: {}", e))?;

    let relay_kind: RelayKind = args
        .relay_kind
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid relay kind: {}", e))?;

    let cancel = CancellationToken::new();

    let bootstrap_peers = psyche_inference_node::load_bootstrap_peers(
        args.bootstrap_peer_file.as_ref(),
        "No bootstrap peer file specified",
    )?;

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
        bootstrap_peers,
        None, // secret key (generate new)
        allowlist::AllowAll,
        metrics.clone(),
        Some(cancel.clone()),
    )
    .await
    .context("Failed to initialize P2P network")?;

    info!("P2P network initialized");
    info!("  Endpoint ID: {}", network.endpoint_id());

    // write endpoint to file if requested
    if let Some(ref endpoint_file) = args.write_endpoint_file {
        let endpoint_addr = network.router().endpoint().addr();
        let content = serde_json::to_string(&endpoint_addr)
            .context("Failed to serialize endpoint address")?;
        fs::write(endpoint_file, content).context("Failed to write endpoint file")?;
        info!("Wrote endpoint to {:?}", endpoint_file);
    }

    // both nodes wait to ensure full mesh connectivity
    info!("Waiting for gossip mesh to stabilize...");
    sleep(Duration::from_secs(3)).await;

    let availability_msg = InferenceGossipMessage::NodeAvailable {
        model_name: Some(format!("test-model-{}", args.node_id)),
        checkpoint_id: None,
        capabilities: vec!["test".to_string()],
    };

    network
        .broadcast(&availability_msg)
        .context("Failed to broadcast availability")?;

    info!("Broadcasted initial availability to network");
    info!("Node {} ready! Press Ctrl+C to shutdown.", args.node_id);
    info!("Watching for other nodes...");

    let mut peer_count = 0;
    let mut rebroadcast_interval = tokio::time::interval(Duration::from_secs(5));
    rebroadcast_interval.tick().await;

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

            _ = rebroadcast_interval.tick() => {
                debug!("Rebroadcasting availability...");
                if let Err(e) = network.broadcast(&availability_msg) {
                    error!("Failed to rebroadcast availability: {:#}", e);
                }
            }

            event = network.poll_next() => {
                match event {
                    Ok(Some(NetworkEvent::MessageReceived((peer_id, msg)))) => {
                        match msg {
                            InferenceGossipMessage::NodeAvailable { model_name, checkpoint_id, capabilities } => {
                                peer_count += 1;
                                info!("PEER DISCOVERED!");
                                info!("  Peer ID: {}", peer_id.fmt_short());
                                info!("  Model: {}", model_name.as_deref().unwrap_or("<idle>"));
                                info!("  Checkpoint: {:?}", checkpoint_id);
                                info!("  Capabilities: {:?}", capabilities);
                                info!("  Total peers seen: {}", peer_count);
                            }
                            InferenceGossipMessage::NodeUnavailable => {
                                info!("Peer {} left the network", peer_id.fmt_short());
                            }
                            InferenceGossipMessage::LoadModel { model_name, model_source } => {
                                info!("LoadModel request from {}: {} ({:?})",
                                      peer_id.fmt_short(), model_name, model_source);
                            }
                            InferenceGossipMessage::ReloadCheckpoint { checkpoint_id, checkpoint_source } => {
                                info!("Checkpoint reload request from {}: {} ({})",
                                      peer_id.fmt_short(), checkpoint_id, checkpoint_source);
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

    info!("Shutting down...");
    network.shutdown().await.ok();
    info!("Shutdown complete");

    Ok(())
}
