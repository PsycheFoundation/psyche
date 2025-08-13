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

pub async fn command_set_future_epoch_rates_run(
    backend: SolanaBackend,
    params: CommandSetFutureEpochRatesParams,
) -> Result<()> {
    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&params.run_id);
    let coordinator_instance_state = backend
        .get_coordinator_instance(&coordinator_instance)
        .await?;

    let coordinator_account = coordinator_instance_state.coordinator_account;
    let set = backend
        .set_future_epoch_rates(
            &params.run_id,
            params.treasurer_index,
            &coordinator_account,
            params.earning_rate,
            params.slashing_rate,
        )
        .await?;

    println!("On run {} with transaction {}:", params.run_id, set);
    println!(" - Set earning rate to {:?}", params.earning_rate);
    println!(" - Set slashing rate to {:?}", params.slashing_rate);
    println!("\n===== Logs =====");
    for log in backend.get_logs(&set).await? {
        println!("{log}");
    }

    Ok(())
}
