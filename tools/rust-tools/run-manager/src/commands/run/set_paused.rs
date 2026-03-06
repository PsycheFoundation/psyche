use crate::commands::Command;
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use psyche_solana_treasurer::logic::RunUpdateParams;

use crate::{SolanaBackend, instructions};

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandSetPaused {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(long, env)]
    pub treasurer_index: Option<u64>,
    #[clap(long, env)]
    pub resume: bool,
}

#[async_trait]
impl Command for CommandSetPaused {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            treasurer_index,
            resume,
        } = self;

        let paused = !resume;
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
                    epoch_earning_rate_total_shared: None,
                    epoch_slashing_rate_per_client: None,
                    paused: Some(paused),
                    client_version: None,
                    data_location: None,
                },
            )
        } else {
            instructions::coordinator_set_paused(
                &run_id,
                &coordinator_account,
                &main_authority,
                paused,
            )
        };

        let signature = backend
            .send_and_retry("Set paused", &[instruction], &[])
            .await?;
        println!("Set pause state to {paused} on run {run_id} with transaction {signature}");

        println!("\n===== Logs =====");
        for log in backend.get_logs(&signature).await? {
            println!("{log}");
        }

        Ok(())
    }
}
