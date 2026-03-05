use anyhow::{Context, Result};
use nvml_wrapper::Nvml;
use psyche_coordinator::model;
use psyche_data_provider::{download_parallelism_data_from_gcs_signed, RunDownClient};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ParallelismConfig {
    pub dp: usize,
    pub tp: usize,
    pub micro_batch_size: usize,
}

// Table format: gpu_type -> num_gpus -> config
type Table = HashMap<String, HashMap<String, ParallelismConfig>>;

/// Auto-detect parallelism settings by downloading parallelism_data.json
/// from GCS (via signed URLs) or HuggingFace, then looking up the config
/// for the detected GPU type and count.
pub async fn lookup(
    checkpoint: &model::Checkpoint,
    run_down_client: Option<&Arc<RunDownClient>>,
    hub_read_token: Option<&str>,
) -> Result<ParallelismConfig> {
    let device_count = tch::Cuda::device_count() as usize;
    if device_count == 0 {
        anyhow::bail!("No GPUs found for parallelism auto-detection");
    }

    let gpu_type = normalize_gpu_name(&get_gpu_type_from_nvml()?);
    info!("Detected {} x {} GPU(s)", device_count, gpu_type);

    let json = download_parallelism_data(checkpoint, run_down_client, hub_read_token).await?;
    let table: Table =
        serde_json::from_str(&json).context("Failed to parse parallelism_data.json")?;

    lookup_in_table(&table, &gpu_type, device_count)
}

fn get_gpu_type_from_nvml() -> Result<String> {
    let nvml = Nvml::init().context("Failed to initialize NVML")?;
    let device = nvml
        .device_by_index(0)
        .context("Failed to get GPU device 0")?;
    device.name().context("Failed to get GPU name")
}

fn normalize_gpu_name(raw_name: &str) -> String {
    let upper = raw_name.to_uppercase();
    if upper.contains("H200") {
        "H200".to_string()
    } else if upper.contains("H100") {
        "H100".to_string()
    } else if upper.contains("A100") {
        "A100".to_string()
    } else if upper.contains("L40S") {
        "L40S".to_string()
    } else if upper.contains("L40") {
        "L40".to_string()
    } else if upper.contains("4090") {
        "RTX4090".to_string()
    } else if upper.contains("3090") {
        "RTX3090".to_string()
    } else {
        raw_name.to_string()
    }
}

async fn download_parallelism_data(
    checkpoint: &model::Checkpoint,
    run_down_client: Option<&Arc<RunDownClient>>,
    hub_read_token: Option<&str>,
) -> Result<String> {
    match checkpoint {
        model::Checkpoint::Gcs(_) | model::Checkpoint::P2PGcs(_) => {
            let client = run_down_client
                .ok_or_else(|| anyhow::anyhow!("RunDownClient required for GCS parallelism lookup"))?;
            info!(
                "Fetching parallelism_data.json from GCS via run-down signed URLs for run {}",
                client.run_id()
            );
            download_parallelism_data_from_gcs_signed(client)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))
        }
        model::Checkpoint::Hub(hub_repo) | model::Checkpoint::P2P(hub_repo) => {
            let repo_id: String = (&hub_repo.repo_id).into();
            info!(
                "Fetching parallelism_data.json from HuggingFace repo '{}'",
                repo_id
            );
            download_from_hub(&repo_id, hub_read_token).await
        }
        _ => anyhow::bail!("Parallelism auto-detection requires Hub or GCS checkpoint type"),
    }
}

async fn download_from_hub(repo_id: &str, token: Option<&str>) -> Result<String> {
    let mut builder = hf_hub::api::tokio::ApiBuilder::new();
    if let Some(token) = token {
        builder = builder.with_token(Some(token.to_string()));
    }
    let api = builder.build()?;
    let repo = api.model(repo_id.to_string());
    let path = repo.get("parallelism_data.json").await.with_context(|| {
        format!(
            "parallelism_data.json not found in HuggingFace repo '{}'",
            repo_id
        )
    })?;
    tokio::fs::read_to_string(path)
        .await
        .context("Failed to read parallelism_data.json")
}

fn lookup_in_table(table: &Table, gpu_type: &str, num_gpus: usize) -> Result<ParallelismConfig> {
    let gpu_configs = table.get(gpu_type).ok_or_else(|| {
        anyhow::anyhow!(
            "No parallelism config for GPU type '{}'. Available: {:?}",
            gpu_type,
            table.keys().collect::<Vec<_>>()
        )
    })?;

    gpu_configs
        .get(&num_gpus.to_string())
        .copied()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No parallelism config for {} x {}. Available counts: {:?}",
                num_gpus,
                gpu_type,
                gpu_configs.keys().collect::<Vec<_>>()
            )
        })
}
