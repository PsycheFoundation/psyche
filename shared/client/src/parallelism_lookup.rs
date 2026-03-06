use anyhow::{Context, Result};
use nvml_wrapper::Nvml;
use psyche_data_provider::{RunDownClient, download_parallelism_data_from_gcs_signed};
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

type Table = HashMap<String, HashMap<String, ParallelismConfig>>;

pub async fn lookup(run_down_client: &Arc<RunDownClient>) -> Result<ParallelismConfig> {
    let device_count = tch::Cuda::device_count() as usize;
    if device_count == 0 {
        anyhow::bail!("No GPUs found for parallelism auto-detection");
    }

    let gpu_type = normalize_gpu_name(&get_gpu_type_from_nvml()?);
    info!("Detected {} x {} GPU(s)", device_count, gpu_type);

    info!(
        "Fetching parallelism_data.json from GCS via run-down signed URLs for run {}",
        run_down_client.run_id()
    );
    let json = download_parallelism_data_from_gcs_signed(run_down_client)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

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
    } else {
        raw_name.to_string()
    }
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
