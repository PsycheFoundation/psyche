// Library exports for run-manager
use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

pub mod commands;
pub mod docker;

// Re-export from psyche-solana-rpc for convenience
pub use psyche_solana_rpc::{SolanaBackend, SolanaBackendRunner, instructions, utils};

/// Load environment variables from a file into host process
/// (needed to read RUN_ID, RPC for querying coordinator)
pub fn load_and_apply_env_file(path: &PathBuf) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read env file: {}", path.display()))?;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            std::env::set_var(key.trim().trim_matches('"'), value.trim().trim_matches('"'));
        }
    }
    Ok(())
}

/// Get a required environment variable
pub fn get_env_var(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("Missing required environment variable: {}", name))
}

/// Load the wallet private key from environment variables.
pub fn load_wallet_key() -> Result<String> {
    if let Ok(raw) = std::env::var("RAW_WALLET_PRIVATE_KEY") {
        Ok(raw)
    } else if let Ok(path) = std::env::var("WALLET_PRIVATE_KEY_PATH") {
        std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read wallet key file: {}", path))
    } else {
        bail!("No wallet private key! Set RAW_WALLET_PRIVATE_KEY or WALLET_PRIVATE_KEY_PATH")
    }
    .map(|s| s.trim().to_string())
}

/// Parse an optional string as a Pubkey.
pub fn parse_optional_pubkey(s: Option<&String>, name: &str) -> Result<Option<Pubkey>> {
    s.map(|s| {
        s.parse::<Pubkey>()
            .with_context(|| format!("Failed to parse {} pubkey: {}", name, s))
    })
    .transpose()
}
