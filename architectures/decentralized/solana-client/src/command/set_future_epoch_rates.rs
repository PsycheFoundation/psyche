use anyhow::Result;
use clap::Args;
use psyche_solana_treasurer::logic::RunUpdateParams;

use crate::{SolanaBackend, instructions};

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandSetFutureEpochRatesParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env)]
    treasurer_index: Option<u64>,
    #[clap(long, env)]
    earning_rate: Option<u64>,
    #[clap(long, env)]
    slashing_rate: Option<u64>,
}

pub async fn command_set_future_epoch_rates_execute(
    backend: SolanaBackend,
    params: CommandSetFutureEpochRatesParams,
) -> Result<()> {
    let CommandSetFutureEpochRatesParams {
        run_id,
        treasurer_index,
        earning_rate,
        slashing_rate,
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
                epoch_earning_rate: earning_rate,
                epoch_slashing_rate: slashing_rate,
                paused: None,
            },
        )
    } else {
        instructions::coordinator_set_future_epoch_rates(
            &run_id,
            &coordinator_account,
            &main_authority,
            earning_rate,
            slashing_rate,
        )
    };

    let signature = backend
        .send_and_retry("Set future epoch rates", &[instruction], &[])
        .await?;
    println!("On run {run_id} with transaction {signature}:");
    println!(" - Set earning rate to {earning_rate:?}");
    println!(" - Set slashing rate to {slashing_rate:?}");

    println!("\n===== Logs =====");
    for log in backend.get_logs(&signature).await? {
        println!("{log}");
    }

    Ok(())
}
