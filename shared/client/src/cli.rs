use crate::{CheckpointConfig, HubUploadInfo, WandBInfo};

use anyhow::{anyhow, bail, Result};
use clap::{ArgAction, Args};
use psyche_eval::tasktype_from_name;
use psyche_network::SecretKey;
use std::path::PathBuf;

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
    println!("Secret key: {}", hex::encode(key.secret().as_bytes()));
    Ok(())
}

#[derive(Args, Debug)]
pub struct TrainArgs {
    /// Path to the clients secret key. Create a new random one running `openssl rand 32 > secret.key`. If not provided a random one will be generated.
    #[clap(short, long, env)]
    pub identity_secret_key_path: Option<PathBuf>,

    /// Sets the port for the client's P2P network participation. If not provided, a random port will be chosen.
    #[clap(short, long, env)]
    pub bind_p2p_port: Option<u16>,

    /// Enables a terminal-based graphical interface for monitoring analytics.
    #[clap(
            long,
            action = ArgAction::Set,
            default_value_t = true,
            default_missing_value = "true",
            num_args = 0..=1,
            require_equals = false,
            env
        )]
    pub tui: bool,

    /// A unique identifier for the training run. This ID allows the client to join a specific active run.
    #[clap(long, env)]
    pub run_id: String,

    #[clap(long, default_value_t = 1, env)]
    pub data_parallelism: usize,

    #[clap(long, default_value_t = 1, env)]
    pub tensor_parallelism: usize,

    #[clap(long, env)]
    pub micro_batch_size: Option<usize>,

    /// If provided, every shared gradient this client sees will be written to this directory.
    #[clap(long, env)]
    pub write_gradients_dir: Option<PathBuf>,

    #[clap(long, env)]
    pub eval_tasks: Option<String>,

    #[clap(long, default_value_t = 0, env)]
    pub eval_fewshot: usize,

    #[clap(long, default_value_t = 42, env)]
    pub eval_seed: u64,

    #[clap(long, env)]
    pub eval_task_max_docs: Option<usize>,

    /// If provided, every model parameters update will be save in this directory after each epoch.
    #[clap(long, env)]
    pub checkpoint_dir: Option<PathBuf>,

    /// Path to the Hugging Face repository containing model data and configuration.
    #[clap(long, env)]
    pub hub_repo: Option<String>,

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
        let checkpoint_upload_info = match (
            &hub_read_token,
            self.hub_repo.clone(),
            self.checkpoint_dir.clone(),
        ) {
            (Some(token), Some(repo), Some(dir)) => Some(CheckpointConfig {
                checkpoint_dir: dir,
                hub_upload: Some(HubUploadInfo {
                    hub_repo: repo,
                    hub_token: token.to_string(),
                }),
            }),
            (None, Some(_), Some(_)) => {
                bail!("hub-repo and checkpoint-dir set, but no HF_TOKEN env variable.")
            }
            (_, Some(_), None) => {
                bail!("--hub-repo was set, but no --checkpoint-dir was passed!")
            }
            (_, None, Some(dir)) => Some(CheckpointConfig {
                checkpoint_dir: dir,
                hub_upload: None,
            }),
            (_, None, _) => None,
        };

        Ok(checkpoint_upload_info)
    }

    pub fn eval_tasks(&self) -> Result<Vec<psyche_eval::Task>> {
        let eval_tasks = match &self.eval_tasks {
            Some(eval_tasks) => {
                let result: Result<Vec<psyche_eval::Task>> = eval_tasks
                    .split(",")
                    .map(|eval_task| {
                        tasktype_from_name(eval_task).map(|task_type| {
                            psyche_eval::Task::new(task_type, self.eval_fewshot, self.eval_seed)
                        })
                    })
                    .collect();
                result?
            }
            None => Vec::new(),
        };
        Ok(eval_tasks)
    }
}

pub fn exercise_sdpa_if_needed() {
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
