use anchor_client::solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result};
use psyche_coordinator::model::{Checkpoint, LLM, Model};
use psyche_solana_coordinator::{
    CoordinatorInstance, coordinator_account_from_bytes, find_coordinator_instance,
};
use solana_client::rpc_client::RpcClient;
use tracing::info;

/// Coordinator client for querying Solana
pub struct CoordinatorClient {
    rpc_client: RpcClient,
    #[allow(dead_code)]
    program_id: Pubkey,
}

impl CoordinatorClient {
    pub fn new(rpc_endpoint: String, program_id: Pubkey) -> Self {
        let rpc_client =
            RpcClient::new_with_commitment(rpc_endpoint, CommitmentConfig::confirmed());
        Self {
            rpc_client,
            program_id,
        }
    }

    // Fetch coordinator data and deserialize into a struct
    pub fn fetch_coordinator_data(&self, run_id: &str) -> Result<CoordinatorInstance> {
        // Derive the coordinator instance PDA
        let coordinator_instance = find_coordinator_instance(run_id);

        let account = self
            .rpc_client
            .get_account(&coordinator_instance)
            .context("RPC error: failed to get account")?;

        let instance = CoordinatorInstance::try_deserialize(&mut account.data.as_slice())
            .context("Failed to deserialize CoordinatorInstance")?;

        Ok(instance)
    }

    pub fn get_docker_tag_for_run(&self, run_id: &str, local_docker: bool) -> Result<String> {
        info!("Querying coordinator for Run ID: {}", run_id);

        let instance = self.fetch_coordinator_data(run_id)?;

        // Fetch the coordinator account to get the client version
        let coordinator_account_data = self
            .rpc_client
            .get_account(&instance.coordinator_account)
            .context("RPC error: failed to get coordinator account")?;

        let coordinator_account = coordinator_account_from_bytes(&coordinator_account_data.data)
            .context("Failed to deserialize CoordinatorAccount")?;

        let client_version = String::from(&coordinator_account.state.client_version);

        info!(
            "Fetched CoordinatorInstance from chain: {{ run_id: {}, coordinator_account: {}, client_version: {} }}",
            instance.run_id, instance.coordinator_account, client_version
        );

        // Depending on how the version is specified in the Coordinator, we should format
        // it accordingly. When specifing a RepoId SHA256, we use
        //      <image_name>@sha256:<repo_id>
        // if not using the RepoId hash, we just want
        //      <image_name>:<version>
        // Also, if using the --local flag (only relevant for testing) the image name is
        // just the local ImageId of the docker image
        let image_name = if client_version.starts_with("sha256:") {
            if local_docker {
                client_version
            } else {
                format!("nousresearch/psyche-client@{}", client_version)
            }
        } else if local_docker {
            format!("psyche-solana-client:{}", client_version)
        } else {
            format!("nousresearch/psyche-client:{}", client_version)
        };

        Ok(image_name)
    }

    /// Get the checkpoint type configured for this run
    pub fn get_checkpoint_type(&self, run_id: &str) -> Result<Checkpoint> {
        let instance = self.fetch_coordinator_data(run_id)?;

        let coordinator_account_data = self
            .rpc_client
            .get_account(&instance.coordinator_account)
            .context("RPC error: failed to get coordinator account")?;

        let coordinator_account = coordinator_account_from_bytes(&coordinator_account_data.data)
            .context("Failed to deserialize CoordinatorAccount")?;

        let Model::LLM(LLM { checkpoint, .. }) = coordinator_account.state.coordinator.model;
        Ok(checkpoint)
    }
}
