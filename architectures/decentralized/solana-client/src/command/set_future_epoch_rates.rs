use std::any;

use anchor_spl::mint;
use anyhow::Result;
use clap::Args;
use futures_util::future::err;
use psyche_solana_treasurer::logic::RunUpdateParams;

use crate::utils::native_amount_to_ui_amount;
use crate::utils::ui_amount_to_native_amount;
use crate::{SolanaBackend, instructions};

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandSetFutureEpochRatesParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env)]
    treasurer_index: Option<u64>,
    #[clap(long, env)]
    earning_rate_total_shared: Option<f64>,
    #[clap(long, env)]
    slashing_rate_per_client: Option<f64>,
}

pub async fn command_set_future_epoch_rates_execute(
    backend: SolanaBackend,
    params: CommandSetFutureEpochRatesParams,
) -> Result<()> {
    let CommandSetFutureEpochRatesParams {
        run_id,
        treasurer_index,
        earning_rate_total_shared,
        slashing_rate_per_client,
    } = params;

    let main_authority = backend.get_payer();

    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
    let coordinator_instance_state = backend
        .get_coordinator_instance(&coordinator_instance)
        .await?;
    let coordinator_account = coordinator_instance_state.coordinator_account;

    let instruction = if let Some(treasurer_index) = backend
        .resolve_treasurer_index(&run_id, treasurer_index)
        .await?
    {
        let treasurer_run_content = backend
            .get_treasurer_run(psyche_solana_treasurer::find_run(treasurer_index))
            .await?;
        let mint_decimals = backend
            .get_mint(&treasurer_run_content.collateral_mint)
            .await?
            .decimals;
        instructions::treasurer_run_update(
            &run_id,
            treasurer_index,
            &coordinator_account,
            &main_authority,
            RunUpdateParams {
                metadata: None,
                config: None,
                model: None,
                progress: None,
                epoch_earning_rate_total_shared: ui_amount_to_native_amount(
                    earning_rate_total_shared,
                    mint_decimals,
                ),
                epoch_slashing_rate_per_client: ui_amount_to_native_amount(
                    slashing_rate_per_client,
                    mint_decimals,
                ),
                paused: None,
                client_version: None,
            },
        )
    } else {
        bail!("Setting earning rate only make sense with a treasurer.");
    };

    let signature = backend
        .send_and_retry("Set future epoch rates", &[instruction], &[])
        .await?;
    println!("On run {run_id} with transaction {signature}:");
    println!(" - Set earning rate to {earning_rate_total_shared:?} (divided between clients)");
    println!(" - Set slashing rate to {slashing_rate_per_client:?} (per failing client)");

    println!("\n===== Logs =====");
    for log in backend.get_logs(&signature).await? {
        println!("{log}");
    }

    Ok(())
}
