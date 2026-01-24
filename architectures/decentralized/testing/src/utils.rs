use std::{fs, sync::Arc, time::Duration};

use anchor_client::{
    Cluster, Program,
    solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair},
};
use psyche_coordinator::{
    RunState,
    model::{Checkpoint, Model},
};
use psyche_core::FixedVec;
use psyche_solana_coordinator::{ClientId, SOLANA_MAX_NUM_PENDING_CLIENTS};
use std::env;
use std::path::PathBuf;

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

    pub async fn get_run_state(&self) -> RunState {
        let coordinator = self.get_coordinator_account().await;
        coordinator.state.coordinator.run_state
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
}

pub struct ConfigBuilder {
    base_config: toml::Value,
    num_clients: usize,
    batch_size: u32,
    architecture: String,
    model: String,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    pub fn new() -> Self {
        let path = env::current_dir().unwrap();
        println!("The current directory is {}", path.display());
        #[cfg(not(feature = "python"))]
        let base_path = "../../../config/solana-test/nano-config.toml";
        #[cfg(feature = "python")]
        let base_path = "../../../config/solana-test/light-config.toml";

        let base_config: toml::Value = fs::read_to_string(base_path)
            .expect("Failed to read base config")
            .parse()
            .expect("Failed to parse TOML");

        Self {
            base_config,
            num_clients: 1,
            batch_size: 4,
            architecture: String::from("HfLlama"),
            model: String::from("pefontana/Nano-Llama"),
        }
    }

    pub fn with_num_clients(mut self, num: usize) -> Self {
        self.num_clients = num;
        self
    }

    pub fn with_architecture(mut self, architecture: &str) -> Self {
        self.architecture = architecture.to_string();
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_batch_size(mut self, batch_size: u32) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn build(mut self) {
        // Apply runtime overrides
        self.set_value("config.min_clients", self.num_clients as u32);
        self.set_value("config.init_min_clients", self.num_clients as u32);

        // This means that every client is a witness
        self.set_value("config.witness_nodes", 0_u32);

        self.set_value("model.LLM.architecture", self.architecture.clone());
        self.set_value("config.global_batch_size_start", self.batch_size);
        self.set_value("config.global_batch_size_end", self.batch_size);
        self.set_value("model.LLM.checkpoint.Hub.repo_id", self.model.clone());

        #[cfg(feature = "python")]
        self.set_value("config.warmup_time", 500);
        #[cfg(feature = "python")]
        self.set_value("config.max_round_train_time", 100);

        let config_content = toml::to_string(&self.base_config).unwrap();
        let config_file_path = PathBuf::from("../../../config/solana-test/test-config.toml");
        fs::write(&config_file_path, config_content).unwrap();
    }

    fn set_value(&mut self, path: &str, value: impl Into<toml::Value>) {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = &mut self.base_config;

        for part in &parts[..parts.len() - 1] {
            current = current.get_mut(part).unwrap();
        }

        current[parts.last().unwrap()] = value.into();
    }
}
