use crate::commands::Command;
use async_trait::async_trait;
use std::time::Duration;

use anyhow::Result;
use clap::Args;
use tokio::time::{MissedTickBehavior, interval};

use crate::{SolanaBackend, instructions};

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandTick {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(long, env, default_value_t = 1000)]
    pub ms_interval: u64,
    #[clap(long, env)]
    pub count: Option<u64>,
}

#[async_trait]
impl Command for CommandTick {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            ms_interval,
            count,
        } = self;

        let ticker = backend.get_payer();

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance)
            .await?;
        let coordinator_account = coordinator_instance_state.coordinator_account;

        let mut interval = interval(Duration::from_millis(ms_interval));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        for _ in 0..count.unwrap_or(u64::MAX) {
            let instruction = instructions::coordinator_tick(
                &coordinator_instance,
                &coordinator_account,
                &ticker,
            );
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
}
