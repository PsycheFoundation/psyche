use anyhow::{Context, Result, bail};
use clap::Args;
use psyche_coordinator::{
    CoordinatorConfig, CoordinatorProgress, get_data_index_for_step,
    model::{Checkpoint, LLMDataLocations, LLMTrainingDataLocation, Model},
};
use psyche_core::FixedVec;
use psyche_solana_treasurer::logic::RunUpdateParams;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{SolanaBackend, instructions};

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
    #[clap(long, env)]
    client_version: Option<String>,
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
        client_version,
    } = params;

    let main_authority = backend.get_payer();

    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
    let coordinator_instance_state = backend
        .get_coordinator_instance(&coordinator_instance)
        .await?;
    let coordinator_account = coordinator_instance_state.coordinator_account;
    let mut coordinator_account_state = backend
        .get_coordinator_account(&coordinator_account)
        .await?;

    let (config, mut model, data_locations) = match config_path {
        Some(config_path) => {
            #[derive(Serialize, Deserialize)]
            struct ModelWrapper {
                #[serde(flatten)]
                pub model: Model,
            }

            #[derive(Serialize, Deserialize)]
            struct State {
                pub config: CoordinatorConfig,
                pub model: ModelWrapper,
            }

            // First, parse without data_locations to get the Model enum
            let state: State = toml::from_str(std::str::from_utf8(
                &std::fs::read(&config_path)
                    .with_context(|| format!("failed to read config toml file {config_path:?}"))?,
            )?)
            .with_context(|| format!("failed to parse config toml file {config_path:?}"))?;

            // Then parse just the data_locations separately
            #[derive(Serialize, Deserialize)]
            struct DataLocationsWrapper {
                pub data_locations: Vec<LLMTrainingDataLocation>,
            }

            #[derive(Serialize, Deserialize)]
            struct LLMSection {
                #[serde(rename = "LLM")]
                pub llm: DataLocationsWrapper,
            }

            #[derive(Serialize, Deserialize)]
            struct ModelSection {
                pub model: LLMSection,
            }

            let data_section: ModelSection = toml::from_str(std::str::from_utf8(
                &std::fs::read(&config_path)
                    .with_context(|| format!("failed to read config toml file {config_path:?}"))?,
            )?)?;

            let data_locs = LLMDataLocations {
                data_locations: FixedVec::from_iter(
                    data_section.model.llm.data_locations.into_iter(),
                ),
            };

            (Some(state.config), Some(state.model.model), Some(data_locs))
        }
        None => (None, None, None),
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

    // update locally to ensure that logic operating on it (e.g. get_data_index_for_step) can read from the new data, not the existing one
    if let Some(config) = config {
        coordinator_account_state.state.coordinator.config = config;
    }

    if let Some(model) = model {
        coordinator_account_state.state.coordinator.model = model;
    }

    if let Some(data_locations) = data_locations {
        coordinator_account_state.state.coordinator.data_locations = data_locations;
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

    let (instructions, data_location_instr) = if let Some(treasurer_index) = backend
        .resolve_treasurer_index(&run_id, treasurer_index)
        .await?
    {
        let mut instructions = Vec::new();
        let mut data_location_instr = Vec::new();

        instructions.push(instructions::treasurer_run_update(
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
                data_location: None,
            },
        ));
        if let Some(data_locations) = data_locations {
            for dl in data_locations.data_locations.iter() {
                data_location_instr.push(instructions::treasurer_run_update(
                    &run_id,
                    treasurer_index,
                    &coordinator_account,
                    &main_authority,
                    RunUpdateParams {
                        metadata: None,
                        config: None,
                        model: None,
                        progress: None,
                        epoch_earning_rate_total_shared: None,
                        epoch_slashing_rate_per_client: None,
                        paused: None,
                        client_version: None,
                        data_location: Some(*dl),
                    },
                ));
            }
        }
        (instructions, data_location_instr)
    } else {
        let mut instructions = Vec::new();
        let mut data_location_instr = Vec::new();
        let data_locations_iter = data_locations.unwrap().iter().cloned().collect::<Vec<_>>();

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
            data_location_instr.push(instructions::clear_data_locations(
                &run_id,
                &coordinator_account,
                &main_authority,
            ));
            for dl in data_locations_iter.iter() {
                data_location_instr.push(instructions::coordinator_update_data_locations(
                    &run_id,
                    &coordinator_account,
                    &main_authority,
                    Some(*dl),
                ));
            }
        }

        if let Some(client_version) = client_version.clone() {
            instructions.push(instructions::coordinator_update_client_version(
                &run_id,
                &coordinator_account,
                &main_authority,
                &client_version,
            ));
        }

        (instructions, data_location_instr)
    };
    let signature = backend
        .send_and_retry("Update config", &instructions, &[])
        .await?;
    println!("Updated config of {run_id} with transaction {signature}");

    let signature = backend
        .send_and_retry("Update data locations", &data_location_instr, &[])
        .await?;

    println!(" - Metadata: {metadata:#?}");
    println!(" - Config: {config:#?}");
    println!(" - Model: {model:#?}");
    println!(" - Data locations: {data_locations:#?}");
    println!(" - Progress: {progress:#?}");
    println!(" - Client version: {client_version:#?}");

    println!("\n===== Logs =====");
    for log in backend.get_logs(&signature).await? {
        println!("{log}");
    }

    Ok(())
}
