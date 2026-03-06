use crate::commands::Command;
use anyhow::Result;
use async_trait::async_trait;
use clap::Args;
use psyche_coordinator::{CoordinatorConfig, model::Model};
use serde::Serialize;

use psyche_solana_rpc::SolanaBackend;

#[derive(Serialize)]
struct State {
    pub config: CoordinatorConfig,
    pub model: Model,
}

#[derive(Debug, Clone, Args)]
#[command()]
pub struct CommandDumpConfig {
    #[clap(short, long, env)]
    pub run_id: String,
}

#[async_trait]
impl Command for CommandDumpConfig {
    async fn execute(self, backend: SolanaBackend) -> Result<()> {
        let coordinator_instance_address =
            psyche_solana_coordinator::find_coordinator_instance(&self.run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance_address)
            .await?;

        let coordinator_account_address = coordinator_instance_state.coordinator_account;
        let coordinator_account_state = backend
            .get_coordinator_account(&coordinator_account_address)
            .await?;

        let state = State {
            config: coordinator_account_state.state.coordinator.config,
            model: coordinator_account_state.state.coordinator.model,
        };

        println!("{}", toml::to_string_pretty(&state)?);

        Ok(())
    }
}
