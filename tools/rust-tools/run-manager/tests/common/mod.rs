// Common test utilities for run-manager integration tests

use anchor_client::{
    Cluster,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{Keypair, Signer},
    },
};
use anyhow::{Context, Result, bail};
use psyche_coordinator::RunState;
use psyche_solana_rpc::SolanaBackend;
use std::sync::Arc;
use std::{
    process::{Command, Stdio},
    time::Duration,
};

/// Test validator Docker guard - stops the validator containers on drop
pub struct TestValidator;

impl TestValidator {
    /// Start the Docker test validator infrastructure
    pub fn start() -> Result<Self> {
        println!("Starting Docker test validator...");

        // Find workspace root (go up from CARGO_MANIFEST_DIR to workspace root)
        let manifest_dir =
            std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;
        let workspace_root = std::path::Path::new(&manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .context("Could not find workspace root")?;
        let compose_file = workspace_root.join("docker/test/docker-compose.yml");

        let output = Command::new("docker")
            .args([
                "compose",
                "-f",
                compose_file.to_str().unwrap(),
                "up",
                "-d",
                "psyche-solana-test-validator",
            ])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .context("Failed to start Docker validator. Is Docker running?")?;

        if !output.status.success() {
            bail!(
                "Failed to start Docker validator: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        // Wait for validator to be healthy
        println!("Waiting for validator to be healthy...");

        // Get expected program IDs from the source
        let authorizer_id = psyche_solana_authorizer::ID.to_string();
        let coordinator_id = psyche_solana_coordinator::ID.to_string();

        println!("Looking for Authorizer program: {}", authorizer_id);
        println!("Looking for Coordinator program: {}", coordinator_id);

        for i in 0..30 {
            let health_check = Command::new("docker")
                .args([
                    "exec",
                    "test-psyche-solana-test-validator-1",
                    "solana",
                    "account",
                    &authorizer_id,
                    "--url",
                    "http://localhost:8899",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();

            if health_check.is_ok() && health_check.unwrap().success() {
                // Verify coordinator is also deployed
                let coord_check = Command::new("docker")
                    .args([
                        "exec",
                        "test-psyche-solana-test-validator-1",
                        "solana",
                        "account",
                        &coordinator_id,
                        "--url",
                        "http://localhost:8899",
                    ])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();

                if coord_check.is_ok() && coord_check.unwrap().success() {
                    println!("Validator is healthy! Both programs deployed.");
                    return Ok(Self);
                } else {
                    println!("Warning: Authorizer found but Coordinator not deployed yet");
                }
            }

            if i % 5 == 0 && i > 0 {
                println!("Still waiting for validator... (attempt {}/30)", i + 1);
            }
            std::thread::sleep(Duration::from_secs(2));
        }

        bail!("Validator did not become healthy in time");
    }
}

impl Drop for TestValidator {
    fn drop(&mut self) {
        println!("Stopping Docker test validator...");
        // Find workspace root for compose file path
        if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
            if let Some(workspace_root) = std::path::Path::new(&manifest_dir)
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
            {
                let compose_file = workspace_root.join("docker/test/docker-compose.yml");
                let _ = Command::new("docker")
                    .args(["compose", "-f", compose_file.to_str().unwrap(), "down"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .output();
            }
        }
    }
}

/// Test client for querying coordinator state
pub struct TestClient {
    backend: SolanaBackend,
    coordinator_instance: Pubkey,
    coordinator_account: Pubkey,
}

impl TestClient {
    pub async fn new(run_id: String, wallet: Arc<Keypair>) -> Result<Self> {
        let cluster = Cluster::Localnet;
        let backend = SolanaBackend::new(cluster, vec![], wallet, CommitmentConfig::confirmed())?;

        tokio::time::sleep(Duration::from_millis(500)).await;

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);

        let coordinator_account = match backend
            .get_coordinator_instance(&coordinator_instance)
            .await
        {
            Ok(instance) => instance.coordinator_account,
            Err(_) => Pubkey::default(),
        };

        Ok(Self {
            backend,
            coordinator_instance,
            coordinator_account,
        })
    }

    pub async fn refresh_coordinator_account(&mut self) -> Result<()> {
        let instance = self
            .backend
            .get_coordinator_instance(&self.coordinator_instance)
            .await?;
        self.coordinator_account = instance.coordinator_account;
        Ok(())
    }

    pub async fn get_run_state(&self) -> Result<RunState> {
        let account = self
            .backend
            .get_coordinator_account(&self.coordinator_account)
            .await?;
        Ok(account.state.coordinator.run_state)
    }

    pub async fn run_exists(&self) -> bool {
        self.backend
            .get_coordinator_instance(&self.coordinator_instance)
            .await
            .is_ok()
    }
}

/// Generate a test keypair and fund it with SOL
pub fn create_test_keypair() -> Result<Arc<Keypair>> {
    let keypair = Keypair::new();
    let output = std::process::Command::new("solana")
        .args([
            "airdrop",
            "100",
            &keypair.pubkey().to_string(),
            "--url",
            "http://127.0.0.1:8899",
        ])
        .output()
        .context("Failed to execute solana airdrop")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Airdrop failed: {}. Tests may fail if account has no balance.",
            error
        );
    }
    println!("Airdropped 100 SOL to {}", keypair.pubkey());

    Ok(Arc::new(keypair))
}
