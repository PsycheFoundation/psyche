use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::PathBuf;
use tokio::signal;
use tracing::{info, warn};

use crate::docker::client::DockerClient;
use crate::docker::coordinator_client::CoordinatorClient;
use crate::get_env_var;
use crate::load_and_apply_env_file;

const RETRY_DELAY_SECS: u64 = 5;
const VERSION_MISMATCH_EXIT_CODE: i32 = 10;

pub struct RunManager {
    env_file: PathBuf,
    wallet_key: String,
    run_id: String,
    local_docker: bool,
    coordinator_client: CoordinatorClient,
    scratch_dir: Option<String>,
    docker_client: DockerClient,
}

#[derive(Debug)]
pub struct Entrypoint {
    pub entrypoint: String,
    pub args: Vec<String>,
}

impl RunManager {
    pub async fn new(
        coordinator_program_id: String,
        env_file: PathBuf,
        local_docker: bool,
    ) -> Result<Self> {
        // Initialize Docker client and verify connection
        let docker_client = DockerClient::new()?;
        docker_client.verify_connection().await?;

        load_and_apply_env_file(&env_file)?;

        let wallet_key =
            if let Ok(raw_wallet_private_key) = std::env::var("RAW_WALLET_PRIVATE_KEY") {
                info!("Using RAW_WALLET_PRIVATE_KEY from command line");
                raw_wallet_private_key
            } else if let Ok(wallet_path) = std::env::var("WALLET_PRIVATE_KEY_PATH") {
                info!("Using WALLET_PRIVATE_KEY_PATH: {wallet_path}");
                fs::read_to_string(wallet_path)?
            } else {
                bail!(
                    "No wallet private key! Must set RAW_WALLET_PRIVATE_KEY or WALLET_PRIVATE_KEY_PATH"
                )
            }
            .trim()
            .to_string();

        let coordinator_program_id = coordinator_program_id
            .parse::<Pubkey>()
            .context("Failed to parse coordinator program ID")?;

        info!("Using coordinator program ID: {}", coordinator_program_id);

        let run_id = get_env_var("RUN_ID")?;
        let rpc = get_env_var("RPC")?;

        let scratch_dir = std::env::var("SCRATCH_DIR").ok();

        let coordinator_client = CoordinatorClient::new(rpc, coordinator_program_id);

        Ok(Self {
            wallet_key,
            run_id,
            coordinator_client,
            env_file,
            local_docker,
            scratch_dir,
            docker_client,
        })
    }

    /// Determine which Docker image to use and pull it if necessary
    async fn prepare_image(&self) -> Result<String> {
        let docker_tag = self
            .coordinator_client
            .get_docker_tag_for_run(&self.run_id, self.local_docker)?;
        info!("Docker tag for run '{}': {}", self.run_id, docker_tag);

        if self.local_docker {
            info!("Using local image (skipping pull): {}", docker_tag);
        } else {
            info!("Pulling image from registry: {}", docker_tag);
            self.docker_client.pull_image(&docker_tag).await?;
        }
        Ok(docker_tag)
    }

    async fn run_container(
        &self,
        image_name: &str,
        entrypoint: &Option<Entrypoint>,
    ) -> Result<String> {
        let client_version = if image_name.contains("sha256:") {
            if self.local_docker {
                image_name
            } else {
                image_name
                    .split('@')
                    .nth(1)
                    .context("Could not split image name")?
            }
        } else {
            image_name
                .split(':')
                .nth(1)
                .context("Could not split image name")?
        };

        // Build environment variables
        let env_vars = vec![
            format!("RAW_WALLET_PRIVATE_KEY={}", &self.wallet_key),
            format!("CLIENT_VERSION={}", client_version),
        ];

        // Determine the actual image to use
        let actual_image = if image_name.contains("sha256:") && self.local_docker {
            // This is a special case for the local version - for ease of use we just
            // run the container using the ImageId SHA256 instead of a full name
            client_version
        } else {
            image_name
        };

        let entrypoint_str = entrypoint.as_ref().map(|e| e.entrypoint.as_str());
        let cmd_args = entrypoint.as_ref().map(|e| e.args.clone());

        self.docker_client
            .run_container(
                actual_image,
                env_vars,
                &self.env_file,
                self.scratch_dir.as_deref(),
                entrypoint_str,
                cmd_args,
            )
            .await
    }

    pub async fn run(&self, entrypoint: Option<Entrypoint>) -> Result<()> {
        loop {
            let docker_tag = self.prepare_image().await?;
            info!("Starting container...");

            let start_time = tokio::time::Instant::now();
            let container_id = self.run_container(&docker_tag, &entrypoint).await?;

            // Race between container completion and Ctrl+C
            let exit_code = tokio::select! {
                result = async {
                    self.docker_client.stream_logs(&container_id).await?;
                    self.docker_client.wait_for_container(&container_id).await
                } => {
                    result?
                },
                _ = signal::ctrl_c() => {
                    info!("\nReceived interrupt signal, cleaning up container...");
                    self.docker_client.stop_and_remove_container(&container_id).await?;
                    info!("Container stopped successfully");
                    return Ok(());
                }
            };

            let duration = start_time.elapsed().as_secs();
            info!(
                "Container exited with code: {} after {} seconds",
                exit_code, duration
            );

            self.docker_client
                .stop_and_remove_container(&container_id)
                .await?;

            // Only retry on version mismatch (exit code 10)
            if exit_code == VERSION_MISMATCH_EXIT_CODE {
                warn!("Version mismatch detected, re-checking coordinator for new version...");
                info!("Waiting {} seconds before retry...", RETRY_DELAY_SECS);
                tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
            } else {
                info!("Container exited with code {}, shutting down", exit_code);
                return Ok(());
            }
        }
    }
}
