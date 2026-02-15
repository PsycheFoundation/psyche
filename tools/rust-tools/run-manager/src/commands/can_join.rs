use crate::commands::Command;
use crate::commands::authorization::Authorization;
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
    /// Authorization type: either a pubkey address or "permissionless" (maps to system program ID)
    #[clap(long, env)]
    pub authorization: Option<Authorization>,
    #[clap(long, env, alias = "wallet", alias = "user", value_name = "PUBKEY")]
    pub address: Pubkey,
}

#[async_trait]
impl Command for CommandCanJoin {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            authorization,
            address,
        } = self;

        let authorizer = authorization.map(|auth| auth.to_pubkey());

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance)
            .await?;

        let authorization = SolanaBackend::find_join_authorization(
            &coordinator_instance_state.join_authority,
            authorizer,
        );
        if backend.get_balance(&authorization).await? == 0 {
            bail!(
                "Authorization does not exist for authorizer: {authorizer:?} (authorization address: {authorization:?}, join authority: {0:?}). Authorizer must be set to grantee pubkey for permissioned runs",
                coordinator_instance_state.join_authority
            );
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

        println!("✓ Can join run {run_id} with pubkey {address}");
        println!("\nTo predownload model and eval tasks before joining, run:");
        println!(
            "  psyche-solana-client predownload --run-id {run_id} --model --eval-tasks <TASKS>"
        );

        Ok(())
    }
}
