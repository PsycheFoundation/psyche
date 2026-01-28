use anyhow::Result;
use hf_hub::{Repo, RepoType};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use tracing::info;

const REMOTE_CONFIG_FILENAME: &str = "parallelism_data.json";

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ParallelismConfig {
    pub dp: usize,
    pub tp: usize,
    pub micro_batch_size: usize,
}

// Table format: gpu_type -> num_gpus -> config
type Table = HashMap<String, HashMap<String, ParallelismConfig>>;

fn get_gpu_type() -> String {
    // Try nvidia-smi first
    let raw_gpu_name = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|s| !s.is_empty())
        // Fallback: read from /proc/driver/nvidia (works in containers without nvidia-smi)
        .or_else(|| {
            std::fs::read_dir("/proc/driver/nvidia/gpus")
                .ok()?
                .filter_map(|e| e.ok())
                .next()
                .and_then(|entry| {
                    let info_path = entry.path().join("information");
                    std::fs::read_to_string(info_path).ok()
                })
                .and_then(|content| {
                    content
                        .lines()
                        .find(|line| line.starts_with("Model:"))
                        .map(|line| line.trim_start_matches("Model:").trim().to_string())
                })
        })
        .unwrap_or_default();

    // Normalize GPU name to match table keys
    if raw_gpu_name.to_uppercase().contains("H200") {
        "H200".to_string()
    } else if raw_gpu_name.to_uppercase().contains("H100") {
        "H100".to_string()
    } else {
        raw_gpu_name
    }
}

/// Try to load parallelism config JSON from the model's HuggingFace repo
fn load_json_from_model_repo(model_repo_id: &str) -> Option<String> {
    let token = std::env::var("HF_TOKEN").ok();

    let api = hf_hub::api::sync::ApiBuilder::new()
        .with_token(token)
        .build()
        .ok()?
        .repo(Repo::new(model_repo_id.to_string(), RepoType::Model));

    let path = api.get(REMOTE_CONFIG_FILENAME).ok()?;
    std::fs::read_to_string(path).ok()
}

/// Lookup config in a table
fn lookup_in_table(table: &Table, gpu_type: &str, num_gpus: usize) -> Option<ParallelismConfig> {
    table
        .get(gpu_type)
        .and_then(|n| n.get(&num_gpus.to_string()))
        .copied()
}

/// Lookup parallelism config from the model's HuggingFace repo
pub fn lookup(model_repo_id: &str) -> Result<ParallelismConfig> {
    let num_gpus = tch::Cuda::device_count() as usize;
    let gpu_type = get_gpu_type();
    info!("Detected {} x {} GPU(s)", num_gpus, gpu_type);

    let raw_json = load_json_from_model_repo(model_repo_id).ok_or_else(|| {
        anyhow::anyhow!(
            "No parallelism_data.json found in model repo '{}'. \
             Add this file to use --parallelism-auto",
            model_repo_id
        )
    })?;

    let table: Table = serde_json::from_str(&raw_json)
        .map_err(|e| anyhow::anyhow!("Failed to parse parallelism_data.json: {}", e))?;

    info!(
        "Using parallelism config from model repo '{}'",
        model_repo_id
    );

    lookup_in_table(&table, &gpu_type, num_gpus)
        .ok_or_else(|| anyhow::anyhow!("No config for {} x {}", num_gpus, gpu_type))
}
