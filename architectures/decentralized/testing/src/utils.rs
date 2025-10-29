use std::{sync::Arc, time::Duration};

use anchor_client::{
    Cluster, Program,
    solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair},
};
use anyhow::Result;
use bollard::Docker;
use futures_util::StreamExt;
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
    /// This method creates a temporary container that mounts the keypair file
    /// to execute the set-paused command with the run owner's authority.
    pub async fn set_paused(
        docker: Arc<Docker>,
        run_id: &str,
        paused: bool,
        keypair_host_path: &str,
    ) -> Result<()> {
        use bollard::container::{Config as ContainerConfig, CreateContainerOptions};
        use bollard::secret::HostConfig;

        let wallet_path = "/tmp/run-owner-keypair.json";
        let rpc = "http://psyche-solana-test-validator:8899";
        let ws_rpc = "ws://psyche-solana-test-validator:8900";

        // Build the set-paused command
        let mut psyche_cmd = format!(
            "psyche-solana-client set-paused --wallet-private-key-path {} --rpc {} --ws-rpc {} --run-id {}",
            wallet_path, rpc, ws_rpc, run_id
        );

        if !paused {
            psyche_cmd.push_str(" --resume");
        }

        // Airdrop the keypair first
        let shell_script = format!(
            "set -ex && \
             solana airdrop 10 --url {} --keypair {} && \
             {}",
            rpc, wallet_path, psyche_cmd
        );
        let cmd = vec!["/bin/sh".to_string(), "-c".to_string(), shell_script];

        let temp_container_name = format!("test-psyche-run-owner-temp-{}", std::process::id());
        let network_name = "test_psyche-test-network";

        println!(
            "Mounting keypair from {} to {}",
            keypair_host_path, wallet_path
        );
        if !std::path::Path::new(keypair_host_path).exists() {
            return Err(anyhow::anyhow!(
                "Keypair file not found at: {}",
                keypair_host_path
            ));
        }
        let binds = vec![format!("{}:{}", keypair_host_path, wallet_path)];

        let host_config = HostConfig {
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: Some(network_name.to_string()),
            binds: Some(binds),
            ..Default::default()
        };

        let env_vars = vec![
            format!("RPC={}", rpc),
            format!("WS_RPC={}", ws_rpc),
            format!("RUN_ID={}", run_id),
        ];

        let options = Some(CreateContainerOptions {
            name: temp_container_name.clone(),
            platform: None,
        });

        let config = ContainerConfig {
            image: Some("psyche-solana-test-client-no-python"),
            cmd: Some(cmd.iter().map(|s| s.as_str()).collect()),
            entrypoint: Some(vec![]), // Clear the default entrypoint to run our command directly, else it will start training
            env: Some(env_vars.iter().map(|s| s.as_str()).collect()),
            host_config: Some(host_config),
            ..Default::default()
        };

        // Create and start the container
        println!("Starting temporary container: {}", temp_container_name);
        docker.create_container(options, config).await?;
        docker
            .start_container::<String>(&temp_container_name, None)
            .await?;

        // Wait for container to complete with timeout
        println!("Waiting for container to complete...");
        use bollard::container::WaitContainerOptions;
        let wait_future = async {
            let mut wait_stream =
                docker.wait_container(&temp_container_name, None::<WaitContainerOptions<String>>);
            while let Some(wait_result) = wait_stream.next().await {
                match wait_result {
                    Ok(result) => {
                        println!("Container finished with status: {:?}", result.status_code);
                        return Ok(result.status_code);
                    }
                    Err(e) => return Err(anyhow::anyhow!("Error waiting for container: {}", e)),
                }
            }
            Ok(0)
        };

        // Add timeout to prevent hanging forever (60s to allow for retries)
        let timed_out = match tokio::time::timeout(Duration::from_secs(60), wait_future).await {
            Ok(Ok(_)) => {
                println!("Container completed successfully");
                false
            }
            Ok(Err(e)) => {
                println!("Container wait error: {}", e);
                true
            }
            Err(_) => {
                println!("Container execution timed out after 60 seconds");
                true
            }
        };

        // Get and print logs (even on timeout to see what went wrong)
        println!("Retrieving container logs...");
        use bollard::container::LogsOptions;
        let logs_options = Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            ..Default::default()
        });

        let mut logs_stream = docker.logs(&temp_container_name, logs_options);
        while let Some(log) = logs_stream.next().await {
            match log {
                Ok(log_output) => print!("  {}", log_output),
                Err(e) => eprintln!("  Error reading logs: {}", e),
            }
        }

        if timed_out {
            return Err(anyhow::anyhow!(
                "Container execution timed out after 30 seconds"
            ));
        }

        // Clean up the temporary container
        use bollard::container::RemoveContainerOptions;
        docker
            .remove_container(
                &temp_container_name,
                Some(RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;

        println!("Set paused state to {} for run {}", paused, run_id);
        Ok(())
    }
}
