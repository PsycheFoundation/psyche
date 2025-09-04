use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use psyche_coordinator::{
    get_data_index_for_step,
    model::{Checkpoint, Model},
    CoordinatorConfig, CoordinatorProgress,
};
use psyche_solana_treasurer::logic::RunUpdateParams;
use serde::{Deserialize, Serialize};

use crate::{instructions, SolanaBackend};

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandUpdateConfigParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env)]
    treasurer_index: Option<u64>,

    #[clap(long, env)]
    config_path: Option<PathBuf>,
    #[clap(long, env)]
    restart_from_step: Option<u32>,
    #[clap(long, env)]
    switch_to_hub: bool,

    // metadata
    #[clap(long)]
    name: Option<String>,
    #[clap(long)]
    description: Option<String>,
    #[clap(long)]
    num_parameters: Option<u64>,
    #[clap(long)]
    vocab_size: Option<u64>,
    // end metadata
}

pub async fn command_update_config_execute(
    backend: SolanaBackend,
    params: CommandUpdateConfigParams,
) -> Result<()> {
    let CommandUpdateConfigParams {
        run_id,
        treasurer_index,
        config_path,
        restart_from_step,
        switch_to_hub,
        name,
        description,
        num_parameters,
        vocab_size,
    } = params;

    let main_authority = backend.get_payer();

    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
    let coordinator_instance_state = backend
        .get_coordinator_instance(&coordinator_instance)
        .await?;
    let coordinator_account = coordinator_instance_state.coordinator_account;
    let coordinator_account_state = backend
        .get_coordinator_account(&coordinator_account)
        .await?;

    let progress = restart_from_step.map(|step| CoordinatorProgress {
        epoch: coordinator_account_state.state.coordinator.progress.epoch,
        step,
        epoch_start_data_index: get_data_index_for_step(
            &coordinator_account_state.state.coordinator,
            step,
        ),
    });

    let (config, mut model) = match config_path {
        Some(config_path) => {
            #[derive(Serialize, Deserialize)]
            struct State {
                pub config: CoordinatorConfig,
                pub model: Model,
            }
            let state: State = toml::from_str(std::str::from_utf8(
                &std::fs::read(&config_path)
                    .with_context(|| format!("failed to read config toml file {config_path:?}"))?,
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

    if metadata.is_none() && config.is_none() && model.is_none() && progress.is_none() {
        bail!("this invocation would not update anything, bailing.")
    }

    let instruction = if let Some(treasurer_index) = backend
        .resolve_treasurer_index(&run_id, treasurer_index)
        .await?
    {
        instructions::treasurer_run_update(
            &run_id,
            treasurer_index,
            &coordinator_account,
            &main_authority,
            RunUpdateParams {
                metadata,
                config,
                model,
                progress,
                epoch_earning_rate: None,
                epoch_slashing_rate: None,
                paused: None,
            },
        )
    } else {
        instructions::coordinator_update(
            &run_id,
            &coordinator_account,
            &main_authority,
            metadata,
            config,
            model,
            progress,
        )
    };
    let signature = backend
        .send_and_retry("Update config", &[instruction], &[])
        .await?;
    println!("Updated config of {run_id} with transaction {signature}");

    println!(" - Metadata: {metadata:#?}");
    println!(" - Config: {config:#?}");
    println!(" - Model: {model:#?}");
    println!(" - Progress: {progress:#?}");

    println!("\n===== Logs =====");
    for log in backend.get_logs(&signature).await? {
        println!("{log}");
    }

    Ok(())
}
