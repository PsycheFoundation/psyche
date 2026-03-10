use anyhow::{Context, Result};
use iroh::EndpointAddr;
use std::{fs, path::PathBuf};
use tracing::info;

pub fn load_bootstrap_peers(
    bootstrap_peer_file: Option<&PathBuf>,
    fallback_message: &str,
) -> Result<Vec<EndpointAddr>> {
    if let Ok(endpoints_json) = std::env::var("PSYCHE_GATEWAY_ENDPOINTS") {
        // JSON array of gateway endpoints
        info!("Reading gateway endpoints from PSYCHE_GATEWAY_ENDPOINTS env var");
        let peers: Vec<EndpointAddr> = serde_json::from_str(&endpoints_json)
            .context("Failed to parse PSYCHE_GATEWAY_ENDPOINTS as JSON array")?;
        info!("Loaded {} gateway endpoint(s) from env var", peers.len());
        for peer in &peers {
            info!("  Gateway: {}", peer.id.fmt_short());
        }
        Ok(peers)
    } else if let Ok(file_path) = std::env::var("PSYCHE_GATEWAY_BOOTSTRAP_FILE") {
        // env var pointing to file
        let peer_file = PathBuf::from(file_path);
        if peer_file.exists() {
            info!(
                "Reading bootstrap peers from PSYCHE_GATEWAY_BOOTSTRAP_FILE: {:?}",
                peer_file
            );
            let content =
                fs::read_to_string(&peer_file).context("Failed to read gateway bootstrap file")?;
            let peers: Vec<EndpointAddr> = serde_json::from_str(&content)
                .context("Failed to parse gateway bootstrap file as JSON array")?;
            info!("Loaded {} gateway endpoint(s) from file", peers.len());
            Ok(peers)
        } else {
            info!("Gateway bootstrap file not found, starting without peers");
            Ok(vec![])
        }
    } else if let Some(peer_file) = bootstrap_peer_file {
        // local testing: CLI argument
        if peer_file.exists() {
            info!("Reading bootstrap peer from {:?}", peer_file);
            let content =
                fs::read_to_string(peer_file).context("Failed to read bootstrap peer file")?;
            // support both single endpoint and array
            if let Ok(peer) = serde_json::from_str::<EndpointAddr>(&content) {
                info!("Bootstrap peer: {}", peer.id.fmt_short());
                Ok(vec![peer])
            } else {
                let peers: Vec<EndpointAddr> = serde_json::from_str(&content)
                    .context("Failed to parse bootstrap peer file")?;
                info!("Loaded {} bootstrap peer(s)", peers.len());
                Ok(peers)
            }
        } else {
            info!("Bootstrap peer file not found, starting without peers");
            Ok(vec![])
        }
    } else {
        info!("{}", fallback_message);
        Ok(vec![])
    }
}
