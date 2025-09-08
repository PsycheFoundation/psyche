use anyhow::Result;
use clap::Args;

use crate::SolanaBackend;

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

    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
    let coordinator_instance_state = backend
        .get_coordinator_instance(&coordinator_instance)
        .await?;

    let coordinator_account = coordinator_instance_state.coordinator_account;
    let set = backend
        .set_future_epoch_rates(
            &run_id,
            treasurer_index,
            &coordinator_account,
            earning_rate,
            slashing_rate,
        )
        .await?;

    println!("On run {run_id} with transaction {set}:");
    println!(" - Set earning rate to {earning_rate:?}");
    println!(" - Set slashing rate to {slashing_rate:?}");
    println!("\n===== Logs =====");
    for log in backend.get_logs(&set).await? {
        println!("{log}");
    }

    Ok(())
}
