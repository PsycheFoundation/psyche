use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result, anyhow};
use clap::Parser;
use psyche_solana_coordinator::{
    CoordinatorInstance, coordinator_account_from_bytes, find_coordinator_instance,
};
use solana_client::rpc_client::RpcClient;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tokio::signal;
use tracing::{error, info, warn};

const MAX_REPEATED_FAILURES: u32 = 5;
const RETRY_DELAY_SECS: u64 = 5;
const RESET_TIME_SECS: u64 = 120;

#[derive(Parser, Debug)]
#[command(name = "run-manager")]
#[command(about = "Manager to download client containers based on a run version")]
struct Args {
    /// Path to wallet private key file
    #[arg(long)]
    wallet_path: PathBuf,

    /// Path to .env file with environment variables
    #[arg(long)]
    env_file: PathBuf,

    /// Coordinator program ID
    #[arg(long, default_value = "HR8RN2TP9E9zsi2kjhvPbirJWA1R6L6ruf4xNNGpjU5Y")]
    coordinator_program_id: String,

    /// Run container in background without streaming logs to console
    #[arg(long, default_value = "false")]
    background: bool,

    /// Use a local Docker image instead of pulling from registry.
    /// If a version is provided, use that specific version. If no version is provided,
    /// query coordinator for the version but skip pulling.
    #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "VERSION")]
    local: Option<String>,
}

/// Load environment variables from a file into host process
/// (needed to read RUN_ID, RPC for querying coordinator)
fn load_and_apply_env_file(path: &PathBuf) -> Result<()> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read env file: {}", path.display()))?;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            std::env::set_var(key.trim(), value.trim());
        }
    }
    Ok(())
}

/// Get a required environment variable
fn get_env_var(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("Missing required environment variable: {}", name))
}

/// Docker manager for container operations
struct DockerManager;

impl DockerManager {
    fn new() -> Result<Self> {
        // Verify docker is available
        Command::new("docker")
            .arg("--version")
            .output()
            .context("Failed to execute docker command. Is Docker installed and accessible?")?;
        Ok(Self)
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

    fn run_container(
        &self,
        image_name: &str,
        env_file: &PathBuf,
        wallet_key: String,
    ) -> Result<String> {
        info!("Creating container from image: {}", image_name);
        let version = image_name.split(':').nth(1).unwrap_or("latest").to_string();

        let mut cmd = Command::new("docker");
        cmd.arg("run")
            .arg("-d") // detached mode
            .arg("--network=host")
            .arg("--shm-size=1g")
            .arg("--privileged")
            .arg("--gpus=all")
            .arg("--device=/dev/infiniband:/dev/infiniband")
            .arg("--env")
            .arg(format!("RAW_WALLET_PRIVATE_KEY={}", wallet_key))
            .arg("--env")
            .arg(format!("PSYCHE_CLIENT_VERSION={}", version));
        cmd.arg("--env-file").arg(env_file);
        cmd.arg(image_name);

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
}

/// Coordinator client for querying Solana
struct CoordinatorClient {
    rpc_client: RpcClient,
    #[allow(dead_code)]
    program_id: Pubkey,
}

impl CoordinatorClient {
    fn new(rpc_endpoint: String, program_id: Pubkey) -> Self {
        let rpc_client = RpcClient::new(rpc_endpoint);
        Self {
            rpc_client,
            program_id,
        }
    }

    // Fetch coordinator data and deserialize into a struct
    fn fetch_coordinator_data(&self, run_id: &str) -> Result<CoordinatorInstance> {
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

    fn get_docker_tag_for_run(&self, local: bool, run_id: &str) -> Result<String> {
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

        let image_name = if local {
            "psyche-solana-client".to_string()
        } else {
            "psyche-client".to_string()
        };
        let docker_tag = format!("{}:{}", image_name, client_version);
        Ok(docker_tag)
    }
}

/// Determine which Docker image to use and pull it if necessary
async fn prepare_image(args: &Args, docker_mgr: &DockerManager) -> Result<String> {
    let docker_tag = match &args.local {
        // --local=version: use explicit version passed by parameter
        Some(version) if !version.is_empty() => {
            let tag = format!("psyche-solana-client:{}", version);
            info!("Using explicit local version: {}", tag);
            tag
        }
        // No --local or --local with no argument: query coordinator
        _ => {
            let run_id = get_env_var("RUN_ID")?;
            let rpc = get_env_var("RPC")?;

            let coordinator_program_id = args
                .coordinator_program_id
                .parse::<Pubkey>()
                .context("Failed to parse coordinator program ID")?;
            info!("Using coordinator program ID: {}", coordinator_program_id);

            let coordinator = CoordinatorClient::new(rpc.clone(), coordinator_program_id);
            let docker_tag = coordinator.get_docker_tag_for_run(args.local.is_some(), &run_id)?;
            info!("Docker tag for run '{}': {}", run_id, docker_tag);
            docker_tag
        }
    };

    // Pull image unless --local flag is present
    if args.local.is_none() {
        docker_mgr.pull_image(&docker_tag)?;
    } else {
        info!("Using local image (skipping pull): {}", docker_tag);
    }

    Ok(docker_tag)
}

async fn run(args: Args) -> Result<()> {
    let wallet_key = std::fs::read_to_string(&args.wallet_path)
        .context("Failed to read wallet file")?
        .trim()
        .to_string();
    let docker_mgr = DockerManager::new()?;
    let mut docker_tag = prepare_image(&args, &docker_mgr).await?;
    let mut repeated_failures = 0;

    loop {
        info!(
            "Starting container (repeated failures: {}/{})",
            repeated_failures, MAX_REPEATED_FAILURES
        );

        let start_time = tokio::time::Instant::now();
        let container_id =
            docker_mgr.run_container(&docker_tag, &args.env_file, wallet_key.clone())?;

        // Race between container completion and Ctrl+C
        let exit_code = tokio::select! {
            result = async {
                if args.background {
                    println!("\nContainer is running in the background.");
                    println!("To view logs: docker logs -f {}", &container_id[..12]);
                    println!("To stop: docker stop {}", &container_id[..12]);
                    docker_mgr.wait_for_container(&container_id)
                } else {
                    docker_mgr.stream_logs(&container_id).await?;
                    docker_mgr.wait_for_container(&container_id)
                }
            } => {
                result?
            },
            _ = signal::ctrl_c() => {
                info!("\nReceived interrupt signal, cleaning up container...");
                docker_mgr.stop_and_remove_container(&container_id)?;
                info!("Container stopped successfully");
                return Ok(());
            }
        };

        let duration = start_time.elapsed().as_secs();
        info!(
            "Container exited with code: {} after {} seconds",
            exit_code, duration
        );

        docker_mgr.stop_and_remove_container(&container_id)?;

        // Reset repeated failures counter if container ran long enough
        if duration >= RESET_TIME_SECS {
            repeated_failures = 0;
            info!(
                "Container ran successfully for {}+ seconds - resetting repeated failure counter",
                RESET_TIME_SECS
            );
        } else {
            repeated_failures += 1;
        }

        // Check if we've exceeded max repeated failures and exit if so
        if repeated_failures >= MAX_REPEATED_FAILURES {
            return Err(anyhow!(
                "Container failed {} times repeatedly",
                MAX_REPEATED_FAILURES
            ));
        }

        // Exit code 10 means version mismatch, so we re-check the coordinator for a new version
        if exit_code == 10 {
            warn!("Version mismatch detected, re-checking coordinator for new version...");
            docker_tag = prepare_image(&args, &docker_mgr).await?;
        } else {
            warn!(
                "Container exited with code {}, retrying with same image...",
                exit_code
            );
        }

        info!("Waiting {} seconds before retry...", RETRY_DELAY_SECS);
        tokio::time::sleep(tokio::time::Duration::from_secs(RETRY_DELAY_SECS)).await;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    load_and_apply_env_file(&args.env_file.clone())?;

    let result = run(args).await;

    if let Err(e) = &result {
        error!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
