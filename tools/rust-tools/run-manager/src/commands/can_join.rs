use crate::commands::Command;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::Result;
use anyhow::bail;
use async_trait::async_trait;
use clap::Args;
use psyche_coordinator::RunState;

use psyche_solana_rpc::SolanaBackend;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandCanJoin {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(long, env)]
    pub authorizer: Option<Pubkey>,
    #[clap(long, env, alias = "wallet", alias = "user", value_name = "PUBKEY")]
    pub address: Pubkey,
}

#[async_trait]
impl Command for CommandCanJoin {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            authorizer,
            address,
        } = self;

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance)
            .await?;

        // If an authorizer is explicitly provided, use it directly.
        // Otherwise, try permissionless first, then fall back to the user's address.
        let authorization = if let Some(auth) = authorizer {
            let authorization = SolanaBackend::find_join_authorization(
                &coordinator_instance_state.join_authority,
                Some(auth),
            );
            if backend.get_balance(&authorization).await? == 0 {
                bail!(
                    "Authorization does not exist for authorizer: {auth} (authorization address: {authorization}, join authority: {}). Authorizer must be set to grantee pubkey for permissioned runs",
                    coordinator_instance_state.join_authority
                );
            }
            authorization
        } else {
            let permissionless_authorization = SolanaBackend::find_join_authorization(
                &coordinator_instance_state.join_authority,
                None,
            );
            if backend.get_balance(&permissionless_authorization).await? > 0 {
                permissionless_authorization
            } else {
                let fallback_authorization = SolanaBackend::find_join_authorization(
                    &coordinator_instance_state.join_authority,
                    Some(address),
                );
                if backend.get_balance(&fallback_authorization).await? == 0 {
                    bail!(
                        "No valid authorization found for {address}. The run is permissioned and you must have a valid authorization. (join authority: {})",
                        coordinator_instance_state.join_authority
                    );
                }
                fallback_authorization
            }
        };
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
        println!("Authorization valid for run id {run_id} using pubkey {address}");

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

        println!("✓ Can join run {run_id} with pubkey {address}");
        println!("\nTo predownload model and eval tasks before joining, run:");
        println!(
            "  psyche-solana-client predownload --run-id {run_id} --model --eval-tasks <TASKS>"
        );

        Ok(())
    }
}
