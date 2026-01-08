use crate::commands::Command;
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use psyche_coordinator::model::HubRepo;
use psyche_core::FixedString;

use psyche_solana_rpc::SolanaBackend;
use psyche_solana_rpc::instructions;

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandCheckpoint {
    #[clap(short, long, env)]
    pub run_id: String,
    #[clap(long, env)]
    pub repo: String,
    #[clap(long, env)]
    pub revision: Option<String>,
}

#[async_trait]
impl Command for CommandCheckpoint {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let Self {
            run_id,
            repo,
            revision,
        } = self;

        let user = backend.get_payer();
        let repo = HubRepo {
            repo_id: FixedString::from_str_truncated(&repo),
            revision: revision
                .clone()
                .map(|x| FixedString::from_str_truncated(&x)),
        };

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance)
            .await?;
        let coordinator_account = coordinator_instance_state.coordinator_account;

        let instruction = instructions::coordinator_checkpoint(
            &coordinator_instance,
            &coordinator_account,
            &user,
            repo,
        );
        let signature = backend
            .send_and_retry("Checkpoint", &[instruction], &[])
            .await?;
        println!("Checkpointed to repo {repo:?} on run {run_id} with transaction {signature}");

        Ok(())
    }
}
