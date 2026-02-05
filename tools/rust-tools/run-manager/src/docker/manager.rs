use anchor_client::solana_sdk::pubkey::Pubkey;
use anyhow::{Context, Result, anyhow, bail};
use psyche_coordinator::model::Checkpoint;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tokio::signal;
use tracing::{error, info, warn};

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
    gcs_credentials_path: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Entrypoint {
    pub entrypoint: String,
    pub args: Vec<String>,
}

impl RunManager {
    pub fn new(
        coordinator_program_id: String,
        env_file: PathBuf,
        local_docker: bool,
    ) -> Result<Self> {
        // Verify docker is available
        Command::new("docker")
            .arg("--version")
            .output()
            .context("Failed to execute docker command. Is Docker installed and accessible?")?;

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

        let scratch_dir = get_env_var("SCRATCH_DIR").ok();

        // Check for GCS credentials file path - will be mounted into container
        let gcs_credentials_path = get_env_var("GOOGLE_CREDENTIALS_FILE_PATH")
            .ok()
            .map(PathBuf::from)
            .and_then(|path| {
                if path.exists() {
                    info!("Found GCS credentials file at: {}", path.display());
                    Some(path)
                } else {
                    warn!(
                        "GOOGLE_CREDENTIALS_FILE_PATH set to {} but file does not exist",
                        path.display()
                    );
                    None
                }
            });

        let coordinator_client = CoordinatorClient::new(rpc, coordinator_program_id);

        Ok(Self {
            wallet_key,
            run_id,
            coordinator_client,
            env_file,
            local_docker,
            scratch_dir,
            gcs_credentials_path,
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
            self.pull_image(&docker_tag)?;
        }
        Ok(docker_tag)
    }

    fn pull_image(&self, image_name: &str) -> Result<()> {
        info!("Pulling image: {}", image_name);

        let mut child = Command::new("docker")
            .arg("pull")
            .arg(image_name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to start docker pull")?;

        // Stream stdout
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => println!("{}", line),
                    Err(e) => error!("Error reading stdout: {}", e),
                }
            }
        }

        let status = child.wait().context("Failed to wait for docker pull")?;
        if !status.success() {
            return Err(anyhow!("Docker pull failed with status: {}", status));
        }

        info!("Successfully pulled image: {}", image_name);
        Ok(())
    }

    fn run_container(&self, image_name: &str, entrypoint: &Option<Entrypoint>) -> Result<String> {
        info!("Creating container from image: {}", image_name);

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

        let mut cmd = Command::new("docker");
        cmd.arg("run")
            .arg("-d")
            .arg("--network=host")
            .arg("--shm-size=1g")
            .arg("--privileged")
            .arg("--gpus=all")
            .arg("--device=/dev/infiniband:/dev/infiniband")
            .arg("--env")
            .arg(format!("RAW_WALLET_PRIVATE_KEY={}", &self.wallet_key))
            .arg("--env")
            .arg(format!("CLIENT_VERSION={}", client_version))
            .arg("--env-file")
            .arg(&self.env_file);

        if let Some(dir) = &self.scratch_dir {
            cmd.arg("--mount")
                .arg(format!("type=bind,src={dir},dst=/scratch"));
        }

        // Mount GCS credentials file if provided and set the env var inside container
        if let Some(creds_path) = &self.gcs_credentials_path {
            let container_creds_path = "/scratch/application_default_credentials.json";
            cmd.arg("--mount")
                .arg(format!(
                    "type=bind,src={},dst={},readonly",
                    creds_path.display(),
                    container_creds_path
                ))
                .arg("--env")
                .arg(format!(
                    "GOOGLE_APPLICATION_CREDENTIALS={}",
                    container_creds_path
                ));
            info!(
                "Mounting GCS credentials from {} to {}",
                creds_path.display(),
                container_creds_path
            );
        }

        if let Some(Entrypoint { entrypoint, .. }) = entrypoint {
            cmd.arg("--entrypoint").arg(entrypoint);
        }

        if image_name.contains("sha256:") && self.local_docker {
            // This is a special case for the local version - for ease of use we just
            // run the container using the ImageId SHA256 instead of a full name
            cmd.arg(client_version);
        } else {
            cmd.arg(image_name);
        }

        if let Some(Entrypoint { args, .. }) = entrypoint {
            cmd.args(args);
        }

        let output = cmd.output().context("Failed to run docker container")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Docker run failed: {}", stderr));
        }

        let container_id = String::from_utf8(output.stdout)
            .context("Failed to parse container ID")?
            .trim()
            .to_string();

        info!("Started container: {}", container_id);
        Ok(container_id)
    }

    async fn stream_logs(&self, container_id: &str) -> Result<()> {
        info!("Streaming logs for container: {}", container_id);

        let mut child = tokio::process::Command::new("docker")
            .arg("logs")
            .arg("-f")
            .arg(container_id)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("Failed to start docker logs")?;

        let status = child
            .wait()
            .await
            .context("Failed to wait for docker logs")?;
        if !status.success() {
            return Err(anyhow!("Docker logs failed with status: {}", status));
        }

        Ok(())
    }

    fn wait_for_container(&self, container_id: &str) -> Result<i32> {
        let output = Command::new("docker")
            .arg("wait")
            .arg(container_id)
            .output()
            .context("Failed to wait for container")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Docker wait failed: {}", stderr));
        }

        let exit_code_str = String::from_utf8(output.stdout)
            .context("Failed to parse exit code")?
            .trim()
            .to_string();

        let exit_code = exit_code_str
            .parse::<i32>()
            .context("Failed to parse exit code as integer")?;

        Ok(exit_code)
    }

    fn stop_and_remove_container(&self, container_id: &str) -> Result<()> {
        info!("Stopping and removing container: {}", container_id);

        // Stop the container
        let stop_output = Command::new("docker")
            .arg("stop")
            .arg(container_id)
            .output()
            .context("Failed to stop container")?;

        if !stop_output.status.success() {
            let stderr = String::from_utf8_lossy(&stop_output.stderr);
            error!("Warning: Docker stop failed: {}", stderr);
        }

        // Remove the container
        let rm_output = Command::new("docker")
            .arg("rm")
            .arg(container_id)
            .output()
            .context("Failed to remove container")?;

        if !rm_output.status.success() {
            let stderr = String::from_utf8_lossy(&rm_output.stderr);
            error!("Warning: Docker rm failed: {}", stderr);
        }

        Ok(())
    }

    /// Validate that required credentials are available based on checkpoint type
    fn validate_credentials(&self) -> Result<()> {
        let checkpoint = self.coordinator_client.get_checkpoint_type(&self.run_id)?;

        match checkpoint {
            Checkpoint::Gcs(_) | Checkpoint::P2PGcs(_) => {
                if self.gcs_credentials_path.is_none() {
                    bail!(
                        "This run uses GCS checkpointing but no GCS credentials found. \
                        Set GOOGLE_CREDENTIALS_FILE_PATH in your env file."
                    );
                }
                info!("GCS credentials validated for checkpoint upload");
            }
            Checkpoint::Hub(_) | Checkpoint::P2P(_) => {
                // HF_TOKEN should be in the env file
                if std::env::var("HF_TOKEN").is_err() {
                    bail!(
                        "This run uses HuggingFace checkpointing but HF_TOKEN is not set. \
                        Please set HF_TOKEN in your environment or env file."
                    );
                }
                info!("HuggingFace token validated for checkpoint upload");
            }
            Checkpoint::Ephemeral | Checkpoint::Dummy(_) => {
                // No credentials needed for ephemeral or dummy checkpoints
                info!("No checkpoint credentials required for this run");
            }
        }

        Ok(())
    }

    pub async fn run(&self, entrypoint: Option<Entrypoint>) -> Result<()> {
        self.validate_credentials()?;

        loop {
            let docker_tag = self.prepare_image().await?;
            info!("Starting container...");

            let start_time = tokio::time::Instant::now();
            let container_id = self.run_container(&docker_tag, &entrypoint)?;

            // Race between container completion and Ctrl+C
            let exit_code = tokio::select! {
                result = async {
                        self.stream_logs(&container_id).await?;
                        self.wait_for_container(&container_id)
                } => {
                    result?
                },
                _ = signal::ctrl_c() => {
                    info!("\nReceived interrupt signal, cleaning up container...");
                    self.stop_and_remove_container(&container_id)?;
                    info!("Container stopped successfully");
                    return Ok(());
                }
            };

            let duration = start_time.elapsed().as_secs();
            info!(
                "Container exited with code: {} after {} seconds",
                exit_code, duration
            );

            self.stop_and_remove_container(&container_id)?;

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
