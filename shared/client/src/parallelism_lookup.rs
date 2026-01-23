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

type Table = HashMap<String, HashMap<String, ParallelismConfig>>;

fn get_gpu_name() -> String {
    Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn lookup(model_repo_id: &str) -> Result<ParallelismConfig> {
    let num_gpus = tch::Cuda::device_count();
    let gpu_name = get_gpu_name();
    info!("Detected {} GPU(s): {}", num_gpus, gpu_name);

    let table: Table = serde_json::from_str(PARALLELISM_DATA)?;

    let gpu_configs = table
        .get(model_repo_id)
        .ok_or_else(|| anyhow::anyhow!("Model '{}' not in parallelism table", model_repo_id))?;

    let config = gpu_configs.get(&num_gpus.to_string()).ok_or_else(|| {
        anyhow::anyhow!(
            "No config for {} GPUs with model '{}'",
            num_gpus,
            model_repo_id
        )
    })?;

    Ok(*config)
}
