use std::{fs, path::Path, sync::Arc, time::Duration};

use anchor_client::{
    ClientError, Cluster, Program,
    solana_sdk::signature::Signature,
    solana_sdk::{
        commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair, signer::Signer,
    },
};
use psyche_coordinator::{
    NUM_STORED_ROUNDS, Round, RunState,
    model::{Checkpoint, Model},
};
use psyche_core::FixedVec;
use psyche_solana_coordinator::{ClientId, SOLANA_MAX_NUM_PENDING_CLIENTS};
use std::env;
use std::path::PathBuf;

/// Write a Solana keypair to a JSON file in the format expected by solana tools
pub fn write_keypair_to_file(keypair: &Keypair, path: &Path) -> std::io::Result<()> {
    let bytes = keypair.to_bytes();
    let json = serde_json::to_string(&bytes.to_vec())?;
    fs::write(path, json)
}

pub struct SolanaTestClient {
    program: Program<Arc<Keypair>>,
    account: Pubkey,
    run_id: String,
    owner_keypair: Arc<Keypair>,
}

impl SolanaTestClient {
    /// Create a new SolanaTestClient.
    /// If owner_keypair is provided, it will be used for signing transactions (like set_paused).
    /// If not provided, a random keypair will be generated (suitable for read-only operations).
    pub async fn new(run_id: String, owner_keypair: Option<Arc<Keypair>>) -> Self {
        let key_pair = owner_keypair.unwrap_or_else(|| Arc::new(Keypair::new()));
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
        let (instance, _) = Pubkey::find_program_address(seeds, &program.id());
        let coordinator_instance: psyche_solana_coordinator::CoordinatorInstance =
            program.account(instance).await.unwrap();
        Self {
            program,
            account: coordinator_instance.coordinator_account,
            run_id,
            owner_keypair: key_pair,
        }
    }

    /// Pause or resume the run. Requires the owner keypair to be the run's main authority.
    pub async fn set_paused(&self, paused: bool) -> Result<Signature, ClientError> {
        let instruction = psyche_solana_rpc::instructions::coordinator_set_paused(
            &self.run_id,
            &self.account,
            &self.owner_keypair.pubkey(),
            paused,
        );
        self.program.request().instruction(instruction).send().await
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
}

pub struct ConfigBuilder {
    base_config: toml::Value,
    num_clients: usize,
    min_clients: Option<usize>,
    batch_size: u32,
    architecture: String,
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
            min_clients: None,
            batch_size: 4,
            architecture: String::from("HfLlama"),
        }
    }

    pub fn with_num_clients(mut self, num: usize) -> Self {
        self.num_clients = num;
        self
    }

    /// Set min_clients
    pub fn with_min_clients(mut self, min: usize) -> Self {
        self.min_clients = Some(min);
        self
    }

    pub fn with_architecture(mut self, architecture: &str) -> Self {
        self.architecture = architecture.to_string();
        self
    }

    pub fn with_batch_size(mut self, batch_size: u32) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn build(mut self) -> PathBuf {
        // Use min_clients if set, otherwise default to num_clients
        let min_clients = self.min_clients.unwrap_or(self.num_clients);

        // Apply runtime overrides
        self.set_value("config.min_clients", min_clients as u32);
        self.set_value("config.init_min_clients", min_clients as u32);

        // This means that every client is a witness
        self.set_value("config.witness_nodes", 0_u32);

        self.set_value("model.LLM.architecture", self.architecture.clone());
        self.set_value("config.global_batch_size_start", self.batch_size);
        self.set_value("config.global_batch_size_end", self.batch_size);

        #[cfg(feature = "python")]
        self.set_value("config.warmup_time", 100);

        let config_content = toml::to_string(&self.base_config).unwrap();
        let config_file_path = PathBuf::from("../../../config/solana-test/test-config.toml");
        fs::write(&config_file_path, config_content).unwrap();

        config_file_path
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
