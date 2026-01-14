mod coordinator_client;
pub mod run_manager;

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Load environment variables from a file into host process
/// (needed to read RUN_ID, RPC for querying coordinator)
fn load_and_apply_env_file(path: &PathBuf) -> Result<()> {
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
fn get_env_var(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("Missing required environment variable: {}", name))
}
