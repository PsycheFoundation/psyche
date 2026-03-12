use crate::commands::Command;
use anchor_spl::associated_token;
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use serde_json::Map;
use serde_json::json;
use serde_json::to_string_pretty;

use psyche_solana_rpc::SolanaBackend;
use psyche_solana_rpc::utils::native_amount_to_ui_amount;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandJsonDumpRun {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(long, env)]
    pub treasurer_index: Option<u64>,
}

#[async_trait]
impl Command for CommandJsonDumpRun {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            treasurer_index,
        } = self;

        let coordinator_instance_address =
            psyche_solana_coordinator::find_coordinator_instance(&run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance_address)
            .await?;
        let coordinator_instance_json = json!({
            "address": coordinator_instance_address.to_string(),
            "join_authority": coordinator_instance_state.join_authority.to_string(),
            "main_authority": coordinator_instance_state.main_authority.to_string(),
        });

        let coordinator_account_address = coordinator_instance_state.coordinator_account;
        let coordinator_account_state = backend
            .get_coordinator_account(&coordinator_account_address)
            .await?;

        let coordinator_account_epoch_clients_alive_json = Map::from_iter(
            coordinator_account_state
                .state
                .coordinator
                .epoch_state
                .clients
                .iter()
                .map(|client| (client.id.to_string(), json!(client.state.to_string()))),
        );
        let coordinator_account_epoch_clients_exited_json = Map::from_iter(
            coordinator_account_state
                .state
                .coordinator
                .epoch_state
                .exited_clients
                .iter()
                .map(|client| (client.id.to_string(), json!(client.state.to_string()))),
        );
        let coordinator_account_clients_json = Map::from_iter(
            coordinator_account_state
                .state
                .clients_state
                .clients
                .iter()
                .map(|client| {
                    (
                        client.id.to_string(),
                        json!({
                            "active": client.active,
                            "earned": client.earned,
                            "slashed": client.slashed,
                        }),
                    )
                }),
        );

        let coordinator_account_clients_max_active = coordinator_account_state
            .state
            .clients_state
            .clients
            .iter()
            .map(|client| client.active)
            .max();
        let coordinator_account_clients_sum_earned = coordinator_account_state
            .state
            .clients_state
            .clients
            .iter()
            .map(|client| client.earned)
            .sum::<u64>();
        let coordinator_account_clients_sum_slashed = coordinator_account_state
            .state
            .clients_state
            .clients
            .iter()
            .map(|client| client.slashed)
            .sum::<u64>();

        let coordinator_account_json = json!({
            "address": coordinator_account_address.to_string(),
            "run_id": coordinator_account_state.state.coordinator.run_id,
            "setup": {
                "client_version": coordinator_account_state.state.client_version,
                "metadata": coordinator_account_state.state.metadata,
                "model": coordinator_account_state.state.coordinator.model,
                "data_locations": coordinator_account_state.state.coordinator.data_locations,
                "config": coordinator_account_state.state.coordinator.config,
            },
            "status": {
                "next_active": coordinator_account_state.state.clients_state.next_active,
                "state": coordinator_account_state.state.coordinator.run_state.to_string(),
                "epoch": coordinator_account_state.state.coordinator.progress.epoch,
                "step": coordinator_account_state.state.coordinator.progress.step,
            },
            "epoch": {
                "clients": {
                    "alive": coordinator_account_epoch_clients_alive_json,
                    "exited": coordinator_account_epoch_clients_exited_json,
                },
                "rates": {
                    "current": {
                        "earning": coordinator_account_state.state.clients_state.current_epoch_rates.earning_rate_total_shared,
                        "slashing": coordinator_account_state.state.clients_state.current_epoch_rates.slashing_rate_per_client,
                    },
                    "future": {
                        "earning": coordinator_account_state.state.clients_state.future_epoch_rates.earning_rate_total_shared,
                        "slashing": coordinator_account_state.state.clients_state.future_epoch_rates.slashing_rate_per_client,
                    },
                }
            },
            "clients": coordinator_account_clients_json,
            "accounting": {
                "max_active": coordinator_account_clients_max_active,
                "sum_earned": coordinator_account_clients_sum_earned,
                "sum_slashed": coordinator_account_clients_sum_slashed,
            },
            "nonce": coordinator_account_state.nonce,
        });

        let treasurer_run_json = if let Some(treasurer_index) = backend
            .resolve_treasurer_index(&run_id, treasurer_index)
            .await?
        {
            let treasurer_run_address = psyche_solana_treasurer::find_run(treasurer_index);
            let treasurer_run_state = backend.get_treasurer_run(&treasurer_run_address).await?;
            let treasurer_run_collateral_address = associated_token::get_associated_token_address(
                &treasurer_run_address,
                &treasurer_run_state.collateral_mint,
            );
            let treasurer_run_collateral_amount = backend
                .get_token_account(&treasurer_run_collateral_address)
                .await?
                .amount;

            let collateral_mint_decimals = backend
                .get_token_mint(&treasurer_run_state.collateral_mint)
                .await?
                .decimals;

            let total_claimed_earned_points = treasurer_run_state.total_claimed_earned_points;
            let total_claimable_earned_points = coordinator_account_clients_sum_earned;
            let total_unclaimed_earned_points =
                total_claimable_earned_points.saturating_sub(total_claimed_earned_points);

            // 1:1 mapping between earned points and collateral amount
            let total_unclaimed_collateral_amount = total_unclaimed_earned_points;
            let burn_collateral_amount_per_epoch = coordinator_account_state
                .state
                .clients_state
                .current_epoch_rates
                .earning_rate_total_shared;

            let total_missing_collateral_amount =
                total_unclaimed_collateral_amount.saturating_sub(treasurer_run_collateral_amount);
            let total_surplus_collateral_amount =
                treasurer_run_collateral_amount.saturating_sub(total_unclaimed_collateral_amount);

            let estimated_funded_epochs_count = if burn_collateral_amount_per_epoch == 0 {
                json!(f64::INFINITY)
            } else {
                json!(
                    total_surplus_collateral_amount as f64
                        / burn_collateral_amount_per_epoch as f64
                )
            };

            Some(json!({
                "address": treasurer_run_address.to_string(),
                "index": treasurer_run_state.index,
                "main_authority": treasurer_run_state.main_authority.to_string(),
                "join_authority": treasurer_run_state.join_authority.to_string(),
                "total_claimed_earned_points": total_claimed_earned_points,
                "total_claimable_earned_points": total_claimable_earned_points,
                "total_unclaimed_earned_points": total_unclaimed_earned_points,
                "collateral_mint": treasurer_run_state.collateral_mint.to_string(),
                "funded_collateral_amount": native_amount_to_ui_amount(
                    treasurer_run_collateral_amount,
                    collateral_mint_decimals
                ),
                "total_missing_collateral_amount": native_amount_to_ui_amount(
                    total_missing_collateral_amount,
                    collateral_mint_decimals
                ),
                "total_surplus_collateral_amount": native_amount_to_ui_amount(
                    total_surplus_collateral_amount,
                    collateral_mint_decimals
                ),
                "burn_collateral_amount_per_epoch": native_amount_to_ui_amount(
                    burn_collateral_amount_per_epoch,
                    collateral_mint_decimals
                ),
                "estimated_funded_epochs_count": estimated_funded_epochs_count,
            }))
        } else {
            None
        };

        println!(
            "{}",
            to_string_pretty(&json!({
                "coordinator_instance": coordinator_instance_json,
                "coordinator_account": coordinator_account_json,
                "treasurer_run": treasurer_run_json,
            }))?
        );

        Ok(())
    }
}
