use crate::commands::Command;
use async_trait::async_trait;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;
use psyche_coordinator::{
    CoordinatorConfig, CoordinatorProgress, get_data_index_for_step,
    model::{CheckpointSource, Model},
    model_extra_data::{CONFIG_PREFIX, CheckpointData, MODEL_CONFIG_FILENAME, ModelExtraData},
};
use psyche_data_provider::upload_json_to_gcs;
use psyche_solana_treasurer::logic::RunUpdateParams;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{SolanaBackend, instructions};

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandUpdateConfig {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(long, env)]
    pub treasurer_index: Option<u64>,

    #[clap(long, env)]
    pub config_path: Option<PathBuf>,
    #[clap(long, env)]
    pub restart_from_step: Option<u32>,
    #[clap(long, env)]
    pub switch_to_hub: bool,

    #[clap(long, env)]
    pub client_version: Option<String>,
    #[clap(long, default_value_t = false, hide = true)]
    pub skip_upload_model_extra_data: bool,

    /// HuggingFace token for uploading to Hub repos (can also use HF_TOKEN env var)
    #[clap(long, env = "HF_TOKEN")]
    pub hub_token: Option<String>,
}

#[async_trait]
impl Command for CommandUpdateConfig {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            treasurer_index,
            config_path,
            restart_from_step,
            switch_to_hub,
            client_version,
            skip_upload_model_extra_data,
            hub_token,
        } = self;

        let main_authority = backend.get_payer();

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance)
            .await?;
        let coordinator_account = coordinator_instance_state.coordinator_account;
        let mut coordinator_account_state = backend
            .get_coordinator_account(&coordinator_account)
            .await?;

        let (config, mut model, model_extra_data) = match config_path {
            Some(config_path) => {
                #[derive(Serialize, Deserialize)]
                struct State {
                    pub config: CoordinatorConfig,
                    pub model: Model,
                    pub model_extra_data: ModelExtraData,
                }
                let state: State = toml::from_str(std::str::from_utf8(
                    &std::fs::read(&config_path).with_context(|| {
                        format!("failed to read config toml file {config_path:?}")
                    })?,
                )?)
                .with_context(|| format!("failed to parse config toml file {config_path:?}"))?;

                (
                    Some(state.config),
                    Some(state.model),
                    Some(state.model_extra_data),
                )
            }
            None => (None, None, None),
        };

        if let (Some(ref model_extra_data), Some(ref mut model)) = (&model_extra_data, &mut model) {
            let Model::LLM(llm) = model;
            llm.checkpoint_data = model_extra_data.checkpoint.to_fixed_vec();
            llm.checkpoint_source = CheckpointSource::Stored;
        }

        model = if switch_to_hub {
            let Model::LLM(mut llm) =
                model.unwrap_or(coordinator_account_state.state.coordinator.model);
            if llm.checkpoint_source == CheckpointSource::P2P {
                llm.checkpoint_source = CheckpointSource::Stored;
            }
            Some(Model::LLM(llm))
        } else {
            model
        };

        // update locally to ensure that logic operating on it (e.g. get_data_index_for_step) can read from the new data, not the existing one
        if let Some(config) = config {
            coordinator_account_state.state.coordinator.config = config;
        }

        if let Some(model) = model {
            coordinator_account_state.state.coordinator.model = model;
        }

        // Upload model extra data to GCS or hub repo depending of the model checkpoint
        if !skip_upload_model_extra_data {
            if let Some(model_extra_data) = model_extra_data {
                let Model::LLM(llm) = &coordinator_account_state.state.coordinator.model;
                match llm.decode_checkpoint() {
                    Some(CheckpointData::Gcs { bucket, .. }) => {
                        let path = format!("{}/{}", CONFIG_PREFIX, MODEL_CONFIG_FILENAME);
                        info!("Uploading model extra data to gs://{}/{}", bucket, path);
                        upload_json_to_gcs(&bucket, &path, &model_extra_data)
                            .await
                            .with_context(|| {
                                format!(
                                    "failed to upload model extra data to gs://{}/{}",
                                    bucket, path
                                )
                            })?;
                        println!("Uploaded model extra data to gs://{}/{}", bucket, path);
                    }
                    Some(CheckpointData::Hub { repo_id, .. }) => {
                        let path = format!("{}/{}", CONFIG_PREFIX, MODEL_CONFIG_FILENAME);
                        psyche_data_provider::upload_model_extra_data_to_hub(
                            &repo_id,
                            &path,
                            &model_extra_data,
                            hub_token.clone(),
                            None,
                        )
                        .await
                        .with_context(|| {
                            format!(
                                "failed to upload model extra data to Hub repo {}/{}",
                                repo_id, path
                            )
                        })?;
                        println!("Uploaded model extra data to Hub repo {}/{}", repo_id, path);
                    }
                    _ => {
                        println!(
                            "Warning: model_extra_data provided but checkpoint is not GCS- or Hub-based, skipping upload"
                        );
                    }
                }
            }
        }

        let progress = restart_from_step.map(|step| CoordinatorProgress {
            epoch: coordinator_account_state.state.coordinator.progress.epoch,
            step,
            epoch_start_data_index: get_data_index_for_step(
                &coordinator_account_state.state.coordinator,
                step,
            ),
        });

        let coordinator_update = config.is_some() || model.is_some() || progress.is_some();
        if !coordinator_update && client_version.is_none() {
            bail!("this invocation would not update anything, bailing.")
        }

        let instructions = if let Some(treasurer_index) = backend
            .resolve_treasurer_index(&run_id, treasurer_index)
            .await?
        {
            vec![instructions::treasurer_run_update(
                &run_id,
                treasurer_index,
                &coordinator_account,
                &main_authority,
                RunUpdateParams {
                    config,
                    model,
                    progress,
                    epoch_earning_rate_total_shared: None,
                    epoch_slashing_rate_per_client: None,
                    paused: None,
                    client_version: client_version.clone(),
                },
            )]
        } else {
            let mut instructions = Vec::new();

            if coordinator_update {
                instructions.push(instructions::coordinator_update(
                    &run_id,
                    &coordinator_account,
                    &main_authority,
                    config,
                    model,
                    progress,
                ));
            }

            if let Some(client_version) = client_version.clone() {
                instructions.push(instructions::coordinator_update_client_version(
                    &run_id,
                    &coordinator_account,
                    &main_authority,
                    &client_version,
                ));
            }

            instructions
        };
        let signature = backend
            .send_and_retry("Update config", &instructions, &[])
            .await?;
        println!("Updated config of {run_id} with transaction {signature}");

        println!(" - Config: {config:#?}");
        println!(" - Model: {model:#?}");
        println!(" - Progress: {progress:#?}");
        println!(" - Client version: {client_version:#?}");

        println!("\n===== Logs =====");
        for log in backend.get_logs(&signature).await? {
            println!("{log}");
        }

        Ok(())
    }
}
