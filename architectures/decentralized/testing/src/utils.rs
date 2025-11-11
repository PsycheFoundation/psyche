use std::{sync::Arc, time::Duration};

use anchor_client::{
    Cluster, Program,
    solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair},
};
use anyhow::Result;
use bollard::Docker;
use psyche_coordinator::{
    NUM_STORED_ROUNDS, Round, RunState,
    model::{Checkpoint, Model},
};
use psyche_core::FixedVec;
use psyche_solana_coordinator::{ClientId, SOLANA_MAX_NUM_PENDING_CLIENTS};

pub struct SolanaTestClient {
    program: Program<Arc<Keypair>>,
    account: Pubkey,
}

impl SolanaTestClient {
    pub async fn new(run_id: String) -> Self {
        let key_pair = Arc::new(Keypair::new());
        tokio::time::sleep(Duration::from_secs(10)).await;
        let cluster = Cluster::Localnet;
        let client = anchor_client::Client::new_with_options(
            cluster.clone(),
            key_pair.clone(),
            CommitmentConfig::confirmed(),
        );
        let program = client.program(psyche_solana_coordinator::ID).unwrap();
        let seeds = &[
            psyche_solana_coordinator::CoordinatorInstance::SEEDS_PREFIX,
            psyche_solana_coordinator::bytes_from_string(&run_id),
        ];
        let (account, _) = Pubkey::find_program_address(seeds, &program.id());
        let instance: psyche_solana_coordinator::CoordinatorInstance =
            program.account(account).await.unwrap();
        Self {
            program,
            account: instance.coordinator_account,
        }
    }

    async fn get_coordinator_account(&self) -> psyche_solana_coordinator::CoordinatorAccount {
        let data = self
            .program
            .rpc()
            .get_account_data(&self.account)
            .await
            .unwrap();
        *psyche_solana_coordinator::coordinator_account_from_bytes(&data).unwrap()
    }

    pub async fn get_checkpoint(&self) -> Checkpoint {
        let coordinator = self.get_coordinator_account().await;
        match coordinator.state.coordinator.model {
            Model::LLM(llm) => llm.checkpoint,
        }
    }

    pub async fn get_clients(
        &self,
    ) -> FixedVec<psyche_solana_coordinator::Client, SOLANA_MAX_NUM_PENDING_CLIENTS> {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.clients_state.clients
    }

    pub async fn get_current_epoch_clients(
        &self,
    ) -> FixedVec<psyche_coordinator::Client<ClientId>, SOLANA_MAX_NUM_PENDING_CLIENTS> {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.epoch_state.clients
    }

    pub async fn get_clients_len(&self) -> usize {
        let clients = self.get_clients().await;
        clients.len()
    }

    pub async fn get_run_state(&self) -> RunState {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.run_state
    }

    pub async fn get_rounds(&self) -> [Round; NUM_STORED_ROUNDS] {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.epoch_state.rounds
    }

    pub async fn get_rounds_head(&self) -> u32 {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.epoch_state.rounds_head
    }

    pub async fn get_current_epoch(&self) -> u16 {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.progress.epoch
    }

    pub async fn get_last_step(&self) -> u32 {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.progress.step
    }

    pub async fn wait_for_run_state(&self, target_state: RunState, timeout_secs: u32) -> bool {
        let mut attempts = 0;
        const MAX_ATTEMPTS_PER_SEC: u32 = 4;
        let max_attempts = timeout_secs * MAX_ATTEMPTS_PER_SEC;

        while attempts < max_attempts {
            let coordinator_state = self.get_run_state().await;
            println!("Current state is {coordinator_state}");

            if coordinator_state == target_state {
                return true;
            }

            attempts += 1;
            tokio::time::sleep(Duration::from_millis(250)).await;
        }

        println!("Timeout waiting for state: {target_state:?}");
        false
    }

    /// Sets the paused state of the run by executing the set-paused command.
    ///
    /// This method creates a temporary container that mounts the keypair file and script
    /// to execute the set-paused command with the run owner's authority.
    pub async fn set_paused(
        docker: Arc<Docker>,
        run_id: &str,
        paused: bool,
        keypair_host_path: &str,
    ) -> Result<()> {
        use bollard::secret::HostConfig;

        let wallet_path = "/tmp/run-owner-keypair.json";
        let script_path = "/tmp/set-paused.sh";
        let rpc = "http://psyche-solana-test-validator:8899";
        let ws_rpc = "ws://psyche-solana-test-validator:8900";

        let temp_container_name = format!("test-psyche-run-owner-temp-{}", std::process::id());
        let network_name = "test_psyche-test-network";

        // Verify keypair exists
        if !std::path::Path::new(keypair_host_path).exists() {
            return Err(anyhow::anyhow!(
                "Keypair file not found at: {}",
                keypair_host_path
            ));
        }

        // Get absolute path to the script from the workspace root
        let script_host_path = std::env::current_dir()?
            .join("../../../scripts/set-paused.sh")
            .canonicalize()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to find script at ../../../scripts/set-paused.sh: {}",
                    e
                )
            })?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to convert script path to string"))?
            .to_string();
        println!(
            "Mounting script from {} to {}",
            script_host_path, script_path
        );

        // Mount both keypair and script
        let binds = vec![
            format!("{}:{}", keypair_host_path, wallet_path),
            format!("{}:{}", script_host_path, script_path),
        ];

        let host_config = HostConfig {
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: Some(network_name.to_string()),
            binds: Some(binds),
            ..Default::default()
        };

        // Run the script with parameters
        let paused_str = if paused { "true" } else { "false" };
        let cmd = vec![
            "sh".to_string(),
            script_path.to_string(),
            run_id.to_string(),
            paused_str.to_string(),
            wallet_path.to_string(),
            rpc.to_string(),
            ws_rpc.to_string(),
        ];

        // Create and start the container
        println!("Starting temporary container: {}", temp_container_name);
        crate::docker_setup::create_and_start_container(
            docker.clone(),
            temp_container_name.clone(),
            "psyche-solana-test-client-no-python",
            vec![],
            host_config,
            Some(vec![]), // Clear the default entrypoint
            Some(cmd),
        )
        .await?;

        // Wait for completion, retrieve logs, and cleanup
        crate::docker_setup::wait_for_container_and_cleanup(docker, &temp_container_name, 60)
            .await?;

        println!("Set paused state to {} for run {}", paused, run_id);
        Ok(())
    }
}
