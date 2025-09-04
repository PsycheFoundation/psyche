use std::time::Duration;

use anyhow::Result;
use clap::Args;
use tokio::time::{interval, MissedTickBehavior};

use crate::{instructions, SolanaBackend};

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandTickParams {
    #[clap(short, long, env)]
    run_id: String,
    #[clap(long, env, default_value_t = 1000)]
    ms_interval: u64,
    #[clap(long, env)]
    count: Option<u64>,
}

pub async fn command_tick_execute(backend: SolanaBackend, params: CommandTickParams) -> Result<()> {
    let CommandTickParams {
        run_id,
        ms_interval,
        count,
    } = params;

    let ticker = backend.get_payer();

    let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
    let coordinator_instance_state = backend
        .get_coordinator_instance(&coordinator_instance)
        .await?;
    let coordinator_account = coordinator_instance_state.coordinator_account;

    let mut interval = interval(Duration::from_millis(ms_interval));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    for _ in 0..count.unwrap_or(u64::MAX) {
        let instruction =
            instructions::coordinator_tick(&coordinator_instance, &coordinator_account, &ticker);
        let signature = backend.send_and_retry("Tick", &[instruction], &[]).await?;
        println!("Ticked run {run_id} with transaction {signature}");

        println!("\n===== Logs =====");
        for log in backend.get_logs(&signature).await? {
            println!("{log}");
        }
        println!();

        interval.tick().await;
    }

    Ok(())
}
