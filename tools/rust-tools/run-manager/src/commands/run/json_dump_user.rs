use crate::commands::Command;
use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_spl::associated_token;
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use serde_json::json;
use serde_json::to_string_pretty;

use psyche_solana_rpc::SolanaBackend;
use psyche_solana_rpc::utils::native_amount_to_ui_amount;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJsonDumpUser {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(long, env)]
    pub treasurer_index: Option<u64>,
    #[clap(long, env, alias = "wallet", alias = "user", value_name = "PUBKEY")]
    pub address: Pubkey,
}

#[async_trait]
impl Command for CommandJsonDumpUser {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            treasurer_index,
            address,
        } = self;

        let balance = backend.get_balance(&address).await?;

        let coordinator_instance_address =
            psyche_solana_coordinator::find_coordinator_instance(&run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance_address)
            .await?;

        let coordinator_account_address = coordinator_instance_state.coordinator_account;
        let coordinator_account_state = backend
            .get_coordinator_account(&coordinator_account_address)
            .await?;

        let mut client_json = None;
        for client in coordinator_account_state.state.clients_state.clients {
            if client.id.signer == address {
                client_json = Some(json!({
                    "active": client.active,
                    "earned": client.earned,
                    "slashed": client.slashed,
                }));
                break;
            }
        }

        let mut epoch_alive_json = None;
        for client in coordinator_account_state
            .state
            .coordinator
            .epoch_state
            .clients
        {
            if client.id.signer == address {
                epoch_alive_json = Some(json!({
                    "state": client.state,
                    "exited_height": client.exited_height,
                }));
                break;
            }
        }

        let mut epoch_exited_json = None;
        for client in coordinator_account_state
            .state
            .coordinator
            .epoch_state
            .exited_clients
        {
            if client.id.signer == address {
                epoch_exited_json = Some(json!({
                    "state": client.state,
                    "exited_height": client.exited_height,
                }));
                break;
            }
        }

        let treasurer_participant_json = if let Some(treasurer_index) = backend
            .resolve_treasurer_index(&run_id, treasurer_index)
            .await?
        {
            let treasurer_run_address = psyche_solana_treasurer::find_run(treasurer_index);
            let treasurer_run_state = backend.get_treasurer_run(&treasurer_run_address).await?;

            let collateral_mint_decimals = backend
                .get_token_mint(&treasurer_run_state.collateral_mint)
                .await?
                .decimals;

            let user_collateral_address = associated_token::get_associated_token_address(
                &address,
                &treasurer_run_state.collateral_mint,
            );
            let user_collateral_amount = backend
                .get_token_account(&user_collateral_address)
                .await
                .map(|account| account.amount)
                .unwrap_or(0);

            let treasurer_participant_address =
                psyche_solana_treasurer::find_participant(&treasurer_run_address, &address);
            let treasurer_participant_state = backend
                .get_treasurer_participant(&treasurer_participant_address)
                .await
                .ok();

            Some(json!({
                "collateral_mint": treasurer_run_state.collateral_mint.to_string(),
                "collateral_amount": native_amount_to_ui_amount(
                    user_collateral_amount,
                    collateral_mint_decimals
                ),
                "claimed_earned_points": treasurer_participant_state
                    .as_ref()
                    .map(|state| state.claimed_earned_points),
                "claimed_collateral_amount": treasurer_participant_state
                    .as_ref()
                    .map(|state| native_amount_to_ui_amount(
                        state.claimed_collateral_amount,
                        collateral_mint_decimals
                    )),
            }))
        } else {
            None
        };

        println!(
            "{}",
            to_string_pretty(&json!({
                "address": address.to_string(),
                "lamports": balance,
                "coordinator_account": {
                    "client": client_json,
                    "epoch_alive": epoch_alive_json,
                    "epoch_exited": epoch_exited_json,
                },
                "treasurer_participant": treasurer_participant_json,
            }))?
        );

        Ok(())
    }
}
