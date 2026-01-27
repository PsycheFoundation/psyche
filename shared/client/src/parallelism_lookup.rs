use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use tracing::info;

const PARALLELISM_DATA: &str = include_str!("parallelism_data.json");

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ParallelismConfig {
    pub dp: usize,
    pub tp: usize,
    pub micro_batch_size: usize,
}

type Table = HashMap<String, HashMap<String, HashMap<String, ParallelismConfig>>>;

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

pub fn lookup(model_repo_id: &str) -> Result<ParallelismConfig> {
    let num_gpus = tch::Cuda::device_count();
    let gpu_type = get_gpu_type();
    info!("Detected {} x {} GPU(s)", num_gpus, gpu_type);

    let table: Table = serde_json::from_str(PARALLELISM_DATA)?;

    let gpu_configs = table
        .get(model_repo_id)
        .ok_or_else(|| anyhow::anyhow!("Model '{}' not in parallelism table", model_repo_id))?;

    let num_gpu_configs = gpu_configs.get(&gpu_type).ok_or_else(|| {
        anyhow::anyhow!(
            "GPU '{}' not in parallelism table for model '{}'",
            gpu_type,
            model_repo_id
        )
    })?;

    let config = num_gpu_configs.get(&num_gpus.to_string()).ok_or_else(|| {
        anyhow::anyhow!(
            "No config for {} x {} with model '{}'",
            num_gpus,
            gpu_type,
            model_repo_id
        )
    })?;

    Ok(*config)
}
