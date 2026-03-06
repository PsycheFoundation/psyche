use crate::commands::Command;
use anyhow::{Result, bail};
use async_trait::async_trait;
use clap::Args;
use psyche_solana_treasurer::logic::RunUpdateParams;

use crate::{SolanaBackend, instructions};
use psyche_solana_rpc::utils::ui_amount_to_native_amount;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandSetFutureEpochRates {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(long, env)]
    pub treasurer_index: Option<u64>,
    #[clap(long, env)]
    pub earning_rate_total_shared: Option<f64>,
    #[clap(long, env)]
    pub slashing_rate_per_client: Option<f64>,
}

#[async_trait]
impl Command for CommandSetFutureEpochRates {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            treasurer_index,
            earning_rate_total_shared,
            slashing_rate_per_client,
        } = self;

        if earning_rate_total_shared.is_none() && slashing_rate_per_client.is_none() {
            bail!(
                "At least one of earning rate or slashing rate must be provided: --earning-rate-total-shared or --slashing-rate-per-client"
            );
        }

        let main_authority = backend.get_payer();

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance)
            .await?;
        let coordinator_account = coordinator_instance_state.coordinator_account;

        let treasurer_index = backend
            .resolve_treasurer_index(&run_id, treasurer_index)
            .await?
            .expect("Setting future epoch rates requires a treasurer.");

        let treasurer_run_content = backend
            .get_treasurer_run(&psyche_solana_treasurer::find_run(treasurer_index))
            .await?;
        let mint_decimals = backend
            .get_token_mint(&treasurer_run_content.collateral_mint)
            .await?
            .decimals;

        let instruction = instructions::treasurer_run_update(
            &run_id,
            treasurer_index,
            &coordinator_account,
            &main_authority,
            RunUpdateParams {
                metadata: None,
                config: None,
                model: None,
                progress: None,
                epoch_earning_rate_total_shared: earning_rate_total_shared
                    .map(|amount| ui_amount_to_native_amount(amount, mint_decimals)),
                epoch_slashing_rate_per_client: slashing_rate_per_client
                    .map(|amount| ui_amount_to_native_amount(amount, mint_decimals)),
                paused: None,
                client_version: None,
                data_location: None,
            },
        );

        if let Some(earning_rate_total_shared) = earning_rate_total_shared {
            println!(
                " - Set earning rate to {earning_rate_total_shared} (divided between clients)"
            );
        }
        if let Some(slashing_rate_per_client) = slashing_rate_per_client {
            println!(" - Set slashing rate to {slashing_rate_per_client} (per failing client)");
        }

        let signature = backend
            .send_and_retry("Set future epoch rates", &[instruction], &[])
            .await?;
        println!("On run {run_id} with transaction {signature}:");

        println!("\n===== Logs =====");
        for log in backend.get_logs(&signature).await? {
            println!("{log}");
        }

        Ok(())
    }
}
