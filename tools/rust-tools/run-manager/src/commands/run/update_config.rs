use crate::commands::Command;
use async_trait::async_trait;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;
use psyche_coordinator::{
    CoordinatorConfig, CoordinatorProgress, get_data_index_for_step,
    model::{Checkpoint, Model},
};
use psyche_solana_treasurer::logic::RunUpdateParams;
use serde::{Deserialize, Serialize};

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

    // metadata
    #[clap(long)]
    pub name: Option<String>,
    #[clap(long)]
    pub description: Option<String>,
    #[clap(long)]
    pub num_parameters: Option<u64>,
    #[clap(long)]
    pub vocab_size: Option<u64>,
    // end metadata
    #[clap(long, env)]
    pub client_version: Option<String>,
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
            name,
            description,
            num_parameters,
            vocab_size,
            client_version,
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

        let (config, mut model) = match config_path {
            Some(config_path) => {
                #[derive(Serialize, Deserialize)]
                struct State {
                    pub config: CoordinatorConfig,
                    pub model: Model,
                }
                let state: State = toml::from_str(std::str::from_utf8(
                    &std::fs::read(&config_path).with_context(|| {
                        format!("failed to read config toml file {config_path:?}")
                    })?,
                )?)
                .with_context(|| format!("failed to parse config toml file {config_path:?}"))?;

                (Some(state.config), Some(state.model))
            }
            None => (None, None),
        };

        model = if switch_to_hub {
            let Model::LLM(mut llm) =
                model.unwrap_or(coordinator_account_state.state.coordinator.model);
            match llm.checkpoint {
                Checkpoint::P2P(hub_repo) | Checkpoint::Dummy(hub_repo) => {
                    llm.checkpoint = Checkpoint::Hub(hub_repo)
                }
                Checkpoint::P2PGcs(gcs_repo) => llm.checkpoint = Checkpoint::Gcs(gcs_repo),
                _ => {}
            }
            Some(Model::LLM(llm))
        } else {
            model
        };

        let metadata = {
            let mut metadata = coordinator_account_state.state.metadata;
            if let Some(name) = name {
                metadata.name = name
                    .as_str()
                    .try_into()
                    .context("run metadata: name failed to convert to FixedString")?;
            }
            if let Some(description) = description {
                metadata.description = description
                    .as_str()
                    .try_into()
                    .context("run metadata: description failed to convert to FixedString")?;
            }
            if let Some(num_parameters) = num_parameters {
                metadata.num_parameters = num_parameters;
            }
            if let Some(vocab_size) = vocab_size {
                metadata.vocab_size = vocab_size;
            }
            // only include if it's different
            (metadata != coordinator_account_state.state.metadata).then_some(metadata)
        };

        // update locally to ensure that logic operating on it (e.g. get_data_index_for_step) can read from the new data, not the existing one
        if let Some(config) = config {
            coordinator_account_state.state.coordinator.config = config;
        }

        if let Some(model) = model {
            coordinator_account_state.state.coordinator.model = model;
        }

        let progress = restart_from_step.map(|step| CoordinatorProgress {
            epoch: coordinator_account_state.state.coordinator.progress.epoch,
            step,
            epoch_start_data_index: get_data_index_for_step(
                &coordinator_account_state.state.coordinator,
                step,
            ),
        });

        let coordinator_update =
            metadata.is_some() || config.is_some() || model.is_some() || progress.is_some();
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
                    metadata,
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
                    metadata,
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

        println!(" - Metadata: {metadata:#?}");
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
