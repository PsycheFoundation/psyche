use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use anyhow::bail;
use clap::Args;
use psyche_client::TrainArgs;
use psyche_coordinator::RunState;
use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::model::Model;

use crate::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandCanJoinParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env)]
    authorizer: Option<Pubkey>,
    #[clap(long, env, action)]
    predownload_model: bool,
    #[clap(long, env, action)]
    predownload_eval_tasks: Option<String>,
    #[clap(long, env, default_value_t = 3)]
    hub_max_concurrent_downloads: usize,
    #[clap(long, env, alias = "wallet", alias = "user", value_name = "PUBKEY")]
    address: Pubkey,
}

pub async fn command_can_join_execute(
    backend: SolanaBackend,
    params: CommandCanJoinParams,
) -> Result<()> {
    let CommandCanJoinParams {
        run_id,
        authorizer,
        predownload_model,
        predownload_eval_tasks,
        hub_max_concurrent_downloads,
        address,
    } = params;

    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
    let coordinator_instance_state = backend
        .get_coordinator_instance(&coordinator_instance)
        .await?;

    let authorization = SolanaBackend::find_join_authorization(
        &coordinator_instance_state.join_authority,
        authorizer,
    );
    if backend.get_balance(&authorization).await? == 0 {
        bail!("Authorization does not exist for authorizer: {authorizer:?} and user: {address}");
    }
    if !backend
        .get_authorization(&authorization)
        .await?
        .is_valid_for(
            &coordinator_instance_state.join_authority,
            &address,
            psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
        )
    {
        bail!("Authorization invalid for run id {run_id} using pubkey {address}");
    }
    println!("authorization valid for run id {run_id} using pubkey {address}");

    let coordinator_account_state = backend
        .get_coordinator_account(&coordinator_instance_state.coordinator_account)
        .await?
        .state
        .coordinator;

    println!(
        "Coordinator: run_state: {}",
        coordinator_account_state.run_state
    );
    let is_paused = matches!(
        coordinator_account_state.run_state,
        RunState::Paused | RunState::Uninitialized
    );
    println!("Coordinator: is_paused: {is_paused}");

    if !is_paused {
        let client_with_our_key = coordinator_account_state
            .epoch_state
            .clients
            .iter()
            .find(|c| c.id.signer == address);
        if client_with_our_key.is_some() {
            bail!(
                "A client with our pubkey {address} is in the current epoch, you can't join with this key!"
            );
        }
    }

    if predownload_model {
        // it would also be reasonable to download the model if we're in WaitingForClients and the checkpoint is not P2P,
        // but that could cause you to miss the transition to Warmup, so we won't do that for now.
        if !is_paused {
            println!("run is in progress, skipping model predownload.");
            return Ok(());
        }

        #[allow(irrefutable_let_patterns)]
        let Model::LLM(model) = coordinator_account_state.model else {
            bail!("model is not an LLM, unsure how to predownload.");
        };

        match model.checkpoint {
            Checkpoint::Ephemeral => {
                bail!("Can't predownload model with ephemeral checkpoint.")
            }
            Checkpoint::Dummy(hub_repo) | Checkpoint::Hub(hub_repo) | Checkpoint::P2P(hub_repo) => {
                let repo_id = hub_repo.repo_id.to_string();
                let revision = hub_repo.revision.map(|s| s.to_string());
                println!(
                    "Predownloading model {repo_id} revision {}",
                    revision.as_ref().unwrap_or(&"main".to_string())
                );
                let hub_read_token = std::env::var("HF_TOKEN").ok();

                // If you pass None as a cache folder, it'll use the env var `HF_HOME`.
                let cache_folder = None;

                psyche_data_provider::download_model_repo_async(
                    &repo_id,
                    revision,
                    cache_folder,
                    hub_read_token,
                    Some(hub_max_concurrent_downloads),
                    true,
                )
                .await?;
            }
            Checkpoint::Gcs(gcs_repo) => {
                let bucket = gcs_repo.bucket.to_string();
                let prefix: Option<String> = gcs_repo.prefix.map(|p| p.to_string());
                println!(
                    "Predownloading model from gs://{}/{}",
                    bucket,
                    prefix.as_deref().unwrap_or("")
                );

                psyche_data_provider::download_model_from_gcs_async(
                    &bucket,
                    prefix.as_deref(),
                    None,
                    true,
                )
                .await;
            }
        };
        println!("Model predownloaded successfully.")
    }

    if let Some(predownload_eval_tasks) = predownload_eval_tasks {
        let _ = TrainArgs::eval_tasks_from_args(&predownload_eval_tasks, 0)?;
        println!("Eval tasks `{predownload_eval_tasks}` predownloaded successfully.");
    }

    Ok(())
}
