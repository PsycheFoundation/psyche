use anyhow::{Result, bail};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

const PARALLELISM_DATA: &str = include_str!("parallelism_data.json");

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ParallelismConfig {
    pub dp: usize,
    pub tp: usize,
    pub micro_batch_size: usize,
}

type Table = HashMap<String, HashMap<String, ParallelismConfig>>;

pub fn get_num_gpus() -> Result<usize> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()?;

    if !output.status.success() {
        bail!("nvidia-smi failed");
    }

    let count = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|s| !s.is_empty())
        .count();

    if count == 0 {
        bail!("No GPUs detected");
    }

    Ok(count)
}

pub fn lookup(model_repo_id: &str, num_gpus: usize) -> Result<ParallelismConfig> {
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
