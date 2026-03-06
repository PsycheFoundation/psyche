mod cli;
mod config;
mod data;
mod train;

use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Parser;
use psyche_coordinator::model::{Checkpoint, LLMTrainingDataLocation, Model};
use psyche_core::OptimizerDefinition;
use psyche_data_provider::DataServerConfig;
use psyche_modeling::Devices;
use psyche_tui::{logging, setup_ctrl_c};
use tracing::info;

use cli::{CliArgs, Commands};
use config::{StateConfig, TrainParams};
use data::{data_provider_from_data_config, data_provider_from_location, local_data_provider};

#[tokio::main]
async fn main() -> Result<()> {
    let logger = logging().init()?;
    psyche_modeling::set_suggested_env_vars();

    let cancel = setup_ctrl_c();
    let cli_args = CliArgs::parse();

    match cli_args.command {
        Some(Commands::PrintAllHelp { markdown }) => {
            assert!(markdown);
            clap_markdown::print_help_markdown::<CliArgs>();
            return Ok(());
        }
        Some(Commands::Config { config, local }) => {
            let content = std::fs::read_to_string(&config)
                .with_context(|| format!("Failed to read config file: {config}"))?;
            let state: StateConfig = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {config}"))?;
            let Model::LLM(llm) = state.model;

            let model_name = match &llm.checkpoint {
                Checkpoint::Hub(repo) | Checkpoint::P2P(repo) | Checkpoint::Dummy(repo) => {
                    repo.repo_id.to_string()
                }
                other => bail!("Unsupported checkpoint type for local training: {other:?}"),
            };

            let device: Devices = local
                .device
                .parse()
                .with_context(|| format!("Invalid device: {}", local.device))?;

            // data.toml is either passed via --data, or auto-detected as `data.toml` next to the config file
            let data_config_path = if let Some(ref data) = local.data {
                Some(std::path::PathBuf::from(data))
            } else if matches!(llm.data_location, LLMTrainingDataLocation::Server(_)) {
                let adjacent = Path::new(&config).parent().map(|p| p.join("data.toml"));
                match adjacent {
                    Some(ref p) if p.exists() => {
                        info!(
                            "Config uses LLMTrainingDataLocation::Server, loading data.toml found next to it at {}",
                            p.display()
                        );
                        adjacent
                    }
                    _ => None,
                }
            } else {
                None
            };

            let mut dataset = if let Some(data_config_path) = data_config_path {
                let data_content =
                    std::fs::read_to_string(&data_config_path).with_context(|| {
                        format!(
                            "Failed to read data config file: {}",
                            data_config_path.display()
                        )
                    })?;
                let mut data_config: DataServerConfig = toml::from_str(&data_content)
                    .with_context(|| {
                        format!(
                            "Failed to parse data config file: {}",
                            data_config_path.display()
                        )
                    })?;

                // Resolve relative dir against the data.toml's parent directory
                if !data_config.dir.is_absolute() {
                    let config_dir = data_config_path.parent().unwrap_or(Path::new(""));
                    data_config.dir = config_dir.join(&data_config.dir);
                }

                data_provider_from_data_config(&data_config).with_context(|| {
                    format!(
                        "Failed to load training data from data config: {}",
                        data_config_path.display()
                    )
                })?
            } else {
                data_provider_from_location(&llm.data_location, llm.max_seq_len, local.seed)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to load training data from location {:?}",
                            llm.data_location
                        )
                    })?
            };

            let params = TrainParams {
                model: model_name,
                sequence_length: llm.max_seq_len as usize,
                optimizer: llm.optimizer,
                lr_schedule: llm.lr_schedule,
                total_steps: state.config.total_steps,
                total_batch: state.config.global_batch_size_end as usize,
                micro_batch: local.micro_batch,
                device,
                grad_accum_in_fp32: local.grad_accum_in_fp32,
                tensor_parallelism: local.tensor_parallelism,
                data_parallelism: local.data_parallelism,
                attn_implementation: local.attn_implementation.map(Into::into),
                start_step: local.start_step,
                architecture: llm.architecture,
                save_path: local.save_path,
            };

            train::run(params, &mut dataset, cancel).await?;
        }
        None => {
            let a = cli_args.run_args;

            let device: Devices = a
                .device
                .parse()
                .with_context(|| format!("Invalid device: {}", a.device))?;

            let clip_grad_norm = match a.max_grad_norm {
                0. => None,
                x => Some(x),
            };

            let optimizer = if a.distro {
                OptimizerDefinition::Distro {
                    clip_grad_norm,
                    compression_decay: a.compression_decay,
                    compression_topk: a.compression_topk,
                    compression_chunk: a.compression_chunk,
                    quantize_1bit: a.distro_quantization,
                    weight_decay: Some(a.weight_decay),
                }
            } else {
                OptimizerDefinition::AdamW {
                    betas: [a.beta1, a.beta2],
                    weight_decay: a.weight_decay,
                    eps: a.eps,
                    clip_grad_norm,
                }
            };

            let lr_schedule = psyche_core::CosineLR::new(
                a.learning_rate,
                a.warmup_steps,
                0.0,
                a.total_steps,
                a.learning_rate / 10.0,
            )
            .into();

            let mut dataset =
                local_data_provider(&a.data_path, a.token_size, a.sequence_length, a.seed)?;

            let params = TrainParams {
                model: a.model,
                sequence_length: a.sequence_length,
                optimizer,
                lr_schedule,
                total_steps: a.total_steps,
                total_batch: a.total_batch,
                micro_batch: a.micro_batch,
                device,
                grad_accum_in_fp32: a.grad_accum_in_fp32,
                tensor_parallelism: a.tensor_parallelism,
                data_parallelism: a.data_parallelism,
                attn_implementation: a.attn_implementation.map(Into::into),
                start_step: a.start_step,
                architecture: a.architecture.into(),
                save_path: a.save_path,
            };

            train::run(params, &mut dataset, cancel).await?;
        }
    }

    logger.shutdown()?;
    Ok(())
}
