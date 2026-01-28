use anyhow::Result;
use hf_hub::{Repo, RepoType};
use nvml_wrapper::Nvml;
use serde::Deserialize;
use std::collections::HashMap;
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

#[derive(Debug)]
struct GpuInfo {
    name: String,
    device_count: u32,
}

fn get_gpu_info() -> Result<GpuInfo> {
    let nvml = Nvml::init()?;
    let device_count = nvml.device_count()?;

    if device_count == 0 {
        anyhow::bail!("No GPUs found!");
    }

    let mut gpu_names = Vec::new();
    for i in 0..device_count {
        let device = nvml.device_by_index(i)?;
        gpu_names.push(device.name()?);
    }

    let first_name = &gpu_names[0];
    if !gpu_names.iter().all(|name| name == first_name) {
        anyhow::bail!(
            "All GPUs must be of the same type, but we have mismatching names: {:?}",
            gpu_names
        );
    }

    Ok(GpuInfo {
        name: gpu_names.pop().unwrap(),
        device_count,
    })
}

fn normalize_gpu_name(raw_name: &str) -> String {
    let upper = raw_name.to_uppercase();
    if upper.contains("H200") {
        "H200".to_string()
    } else if upper.contains("H100") {
        "H100".to_string()
    } else {
        raw_name.to_string()
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
    let gpu_info = get_gpu_info()?;
    let gpu_type = normalize_gpu_name(&gpu_info.name);
    info!("Detected {} x {} GPU(s)", gpu_info.device_count, gpu_type);

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

    lookup_in_table(&table, &gpu_type, gpu_info.device_count as usize)
        .ok_or_else(|| anyhow::anyhow!("No config for {} x {}", gpu_info.device_count, gpu_type))
}
