use crate::{CheckpointConfig, WandBInfo};

use crate::UploadInfo;
use anyhow::{Context, Result, anyhow, bail};
use clap::Args;
use psyche_coordinator::model_extra_data::ModelExtraData;
use psyche_data_provider::{GcsUploadInfo, HubUploadInfo};
use psyche_eval::tasktype_from_name;
use psyche_modeling::Devices;
use psyche_network::{DiscoveryMode, RelayKind, SecretKey};
use psyche_tui::LogOutput;
use std::{path::PathBuf, time::Duration};

pub fn read_identity_secret_key(
    identity_secret_key_path: Option<&PathBuf>,
) -> Result<Option<SecretKey>> {
    let raw_identity_secret_key = std::env::var("RAW_IDENTITY_SECRET_KEY").ok();
    let bytes: [u8; 32] = match (raw_identity_secret_key, identity_secret_key_path) {
        (None, None) => return Ok(None),
        (Some(raw), None) => {
            let vals = hex::decode(raw)?;
            let l = vals.len();
            vals.try_into().map_err(|_| {
                anyhow!(
                    "invalid raw identity secret key, expected 32 bytes, got {}",
                    l
                )
            })?
        }

        (None, Some(key_file)) => std::fs::read(key_file)?
            .try_into()
            .map_err(|_| anyhow!("key file {key_file:?} was not 32 bytes long."))?,

        _ => unreachable!(),
    };
    Ok(Some(SecretKey::from_bytes(&bytes)))
}

pub fn print_identity_keys(key: Option<&PathBuf>) -> Result<()> {
    let key = read_identity_secret_key(key)?.ok_or_else(|| {
        anyhow!("Use --identity-secret-key-path or use `RAW_IDENTITY_SECRET_KEY` env variable")
    })?;
    println!("Public key: {}", key.public());
    println!("Secret key: {}", hex::encode(key.to_bytes()));
    Ok(())
}

fn parse_trim_quotes(s: &str) -> Result<String, String> {
    Ok(s.trim_matches('"').to_string())
}

#[derive(Args, Debug)]
pub struct TrainArgs {
    /// Path to the clients secret key. Create a new random one running `openssl rand 32 > secret.key`. If not provided a random one will be generated.
    #[clap(short, long, env)]
    pub identity_secret_key_path: Option<PathBuf>,

    /// Sets the port for the client's P2P network participation. If not provided, a random port will be chosen.
    #[clap(long, env)]
    pub bind_p2p_port: Option<u16>,

    /// Sets the network interface for the client's P2P network participation. If not provided, will bind to all interfaces.
    #[clap(long, env)]
    pub bind_p2p_interface: Option<String>,

    /// What relays to use - public n0 or the private Psyche ones
    #[clap(long, env, default_value = "psyche")]
    pub iroh_relay: RelayKind,

    /// What discovery to use - public n0 or local
    #[clap(long, env, default_value = "n0")]
    pub iroh_discovery: DiscoveryMode,

    /// Sets clients logs interface
    /// tui: Enables a terminal-based graphical interface for monitoring analytics.
    /// console: standard logs
    /// json: standard logs with json format
    #[clap(
        long,
        env,
        default_value_t = LogOutput::TUI,
        value_enum,
        ignore_case = true
    )]
    pub logs: LogOutput,

    /// An auth header string for an opentelemetry endpoint. Used for both logging and metrics.
    #[clap(long, env)]
    pub oltp_auth_header: Option<String>,

    /// A URL for sending opentelemetry metrics. probably ends in /v1/metrics
    #[clap(long, env)]
    pub oltp_metrics_url: Option<String>,

    /// A URL for sending opentelemetry traces. probably ends in /v1/traces
    #[clap(long, env)]
    pub oltp_tracing_url: Option<String>,

    /// A URL for sending opentelemetry logs. probably ends in /v1/logs
    #[clap(long, env)]
    pub oltp_logs_url: Option<String>,

    /// how often to report metrics thru opentelemetry
    #[clap(long, env,
    default_value = "60.0",
    value_parser = parse_duration_from_seconds)]
    pub oltp_report_interval: Duration,

    /// If present, output some metrics & stats via this TCP port in JSON format. Useful for debugging or local integration.
    #[clap(long, env)]
    pub metrics_local_port: Option<u16>,

    /// A unique identifier for the training run. This ID allows the client to join a specific active run.
    #[clap(long, env, value_parser = parse_trim_quotes)]
    pub run_id: String,

    #[clap(long, default_value_t = 1, env)]
    pub data_parallelism: usize,

    #[clap(long, default_value_t = 1, env)]
    pub tensor_parallelism: usize,

    #[clap(long, env, default_value_t = 1)]
    pub micro_batch_size: usize,

    /// If provided, every shared gradient this client sees will be written to this directory.
    #[clap(long, env)]
    pub write_gradients_dir: Option<PathBuf>,

    #[clap(long, env)]
    pub eval_tasks: Option<String>,

    #[clap(long, default_value_t = 42, env)]
    pub eval_seed: u64,

    #[clap(long, env)]
    pub eval_task_max_docs: Option<usize>,

    // enable the execution of the model prompting task
    #[clap(long, env)]
    pub prompt_task: bool,

    /// If provided, every model parameters update will be save in this directory after each epoch.
    #[clap(long, env)]
    pub checkpoint_dir: Option<PathBuf>,

    /// Path to the Hugging Face repository containing model data and configuration.
    #[clap(long, env)]
    pub hub_repo: Option<String>,

    /// Name of the GCS bucket containing model data and configuration.
    #[clap(long, env)]
    pub gcs_bucket: Option<String>,

    /// Prefix within the GCS bucket for model data and configuration.
    #[clap(long, env)]
    pub gcs_prefix: Option<String>,

    #[clap(long, env, default_value_t = 3)]
    pub hub_max_concurrent_downloads: usize,

    #[clap(long, env)]
    pub wandb_project: Option<String>,

    #[clap(long, env)]
    pub wandb_run: Option<String>,

    #[clap(long, env)]
    pub wandb_group: Option<String>,

    #[clap(long, env)]
    pub wandb_entity: Option<String>,

    #[clap(long, env)]
    pub write_log: Option<PathBuf>,

    #[clap(long, env)]
    pub optim_stats_steps: Option<u32>,

    #[clap(long, default_value_t = false, env)]
    pub grad_accum_in_fp32: bool,

    #[clap(long, env)]
    pub dummy_training_delay_secs: Option<u64>,

    #[clap(long, default_value_t = 8, env)]
    pub max_concurrent_parameter_requests: usize,

    #[clap(long, default_value_t = 4, env)]
    pub max_concurrent_downloads: usize,

    #[arg(
        long,
        help = "Device(s) to use: auto, cpu, mps, cuda, cuda:N, cuda:X,Y,Z",
        default_value = "auto"
    )]
    pub device: Devices,

    #[clap(long, env)]
    pub sidecar_port: Option<u16>,

    #[clap(long, default_value_t = true, env)]
    pub delete_old_steps: bool,

    #[clap(long, default_value_t = 3, env)]
    pub keep_steps: u32,

    /// Path to a TOML config file. If provided, uses this config instead of fetching from the remote repo.
    /// Only meant for testing/debugging.
    #[clap(long, env, hide = true)]
    pub model_extra_data_toml: Option<PathBuf>,
}

impl TrainArgs {
    pub fn wandb_info(&self, run_name: String) -> Result<Option<WandBInfo>> {
        let wandb_info = match std::env::var("WANDB_API_KEY") {
            Ok(wandb_api_key) => Some(WandBInfo {
                project: self.wandb_project.clone().unwrap_or("psyche".to_string()),
                run: self.wandb_run.clone().unwrap_or(run_name),
                entity: self.wandb_entity.clone(),
                api_key: wandb_api_key,
                group: self.wandb_group.clone(),
            }),
            Err(_) => {
                match self.wandb_entity.is_some()
                    || self.wandb_run.is_some()
                    || self.wandb_project.is_some()
                    || self.wandb_group.is_some()
                {
                    true => bail!(
                        "WANDB_API_KEY environment variable must be set for wandb integration"
                    ),
                    false => None,
                }
            }
        };
        Ok(wandb_info)
    }

    pub fn checkpoint_config(&self) -> Result<Option<CheckpointConfig>> {
        let hub_read_token = std::env::var("HF_TOKEN").ok();

        if self.hub_repo.is_some() && self.gcs_bucket.is_some() {
            bail!("Use either GCS or HF hub for checkpoint uploads, not both.");
        }

        let checkpoint_dir = match &self.checkpoint_dir {
            Some(dir) => dir,
            None => {
                if self.hub_repo.is_some() || self.gcs_bucket.is_some() {
                    bail!(
                        "--hub-repo or --gcs-bucket was set, but no --checkpoint-dir was passed!"
                    );
                }
                return Ok(None);
            }
        };

        let upload_info = self.build_upload_info(&hub_read_token)?;

        if upload_info.is_some() && self.keep_steps == 0 {
            bail!(
                "keep_steps must be >= 1 for checkpoint uploads (got {})",
                self.keep_steps
            );
        }

        Ok(Some(CheckpointConfig {
            checkpoint_dir: checkpoint_dir.clone(),
            upload_info,
            delete_old_steps: self.delete_old_steps,
            keep_steps: self.keep_steps,
        }))
    }

    fn build_upload_info(&self, hub_token: &Option<String>) -> Result<Option<UploadInfo>> {
        if let Some(repo) = &self.hub_repo {
            return self.build_hub_upload_info(repo, hub_token);
        }

        if let Some(bucket) = &self.gcs_bucket {
            return self.build_gcs_upload_info(bucket);
        }

        Ok(None)
    }

    fn build_hub_upload_info(
        &self,
        repo: &str,
        token: &Option<String>,
    ) -> Result<Option<UploadInfo>> {
        let token = token.as_ref().ok_or_else(|| {
            anyhow::anyhow!("hub-repo and checkpoint-dir set, but no HF_TOKEN env variable.")
        })?;

        Ok(Some(UploadInfo::Hub(HubUploadInfo {
            hub_repo: repo.to_string(),
            hub_token: token.to_string(),
        })))
    }

    fn build_gcs_upload_info(&self, bucket: &str) -> Result<Option<UploadInfo>> {
        Ok(Some(UploadInfo::Gcs(GcsUploadInfo {
            gcs_bucket: bucket.to_string(),
            gcs_prefix: self.gcs_prefix.clone(),
        })))
    }

    pub fn eval_tasks(&self) -> Result<Vec<psyche_eval::Task>> {
        let eval_tasks = match &self.eval_tasks {
            Some(eval_tasks) => Self::eval_tasks_from_args(eval_tasks, self.eval_seed)?,
            None => Vec::new(),
        };
        Ok(eval_tasks)
    }

    pub fn eval_tasks_from_args(
        eval_tasks: &str,
        eval_seed: u64,
    ) -> Result<Vec<psyche_eval::Task>> {
        let result: Result<Vec<psyche_eval::Task>> = eval_tasks
            .split(",")
            .map(|eval_task| {
                let fewshot = match eval_task {
                    "mmlu_pro" => 5,
                    _ => 0,
                };
                tasktype_from_name(eval_task)
                    .map(|task_type| psyche_eval::Task::new(task_type, fewshot, eval_seed))
            })
            .collect();
        result
    }

    pub fn model_extra_data_override(&self) -> Result<Option<ModelExtraData>> {
        let Some(path) = &self.model_extra_data_toml else {
            return Ok(None);
        };

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read model extra data TOML file {:?}", path))?;

        let toml_value: toml::Value = toml::from_str(&content)
            .with_context(|| format!("failed to parse TOML file {:?}", path))?;

        let model_extra_data_table = toml_value
            .get("model_extra_data")
            .ok_or_else(|| anyhow::anyhow!("missing [model_extra_data] section in {:?}", path))?;

        let config: ModelExtraData =
            model_extra_data_table.clone().try_into().with_context(|| {
                format!(
                    "failed to deserialize model_extra_data from TOML file {:?}",
                    path
                )
            })?;

        Ok(Some(config))
    }
}

pub fn prepare_environment() {
    psyche_modeling::set_suggested_env_vars();

    #[cfg(target_os = "windows")]
    {
        // this is a gigantic hack to cover that called sdpa prints out
        // "Torch was not compiled with flash attention." via TORCH_WARN
        // on Windows, which screws with the TUI.
        // it's done once (really TORCH_WARN_ONCE), so elicit that behavior
        // before starting anything else
        use tch::Tensor;
        let device = tch::Device::Cuda(0);
        let _ = Tensor::scaled_dot_product_attention::<Tensor>(
            &Tensor::from_slice2(&[[0.]]).to(device),
            &Tensor::from_slice2(&[[0.]]).to(device),
            &Tensor::from_slice2(&[[0.]]).to(device),
            None,
            0.0,
            false,
            None,
        );
    }
}

fn parse_duration_from_seconds(s: &str) -> Result<Duration, String> {
    s.parse::<f64>()
        .map_err(|e| format!("Invalid number: {e}"))
        .and_then(|secs| {
            if secs < 0.0 {
                Err("Duration cannot be negative".to_string())
            } else {
                Ok(Duration::from_secs_f64(secs))
            }
        })
}
