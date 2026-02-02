use bollard::{
    Docker,
    container::{
        Config, CreateContainerOptions, KillContainerOptions, ListContainersOptions, LogsOptions,
        RemoveContainerOptions, WaitContainerOptions,
    },
    models::DeviceRequest,
    secret::{ContainerSummary, HostConfig},
};
use futures_util::StreamExt;
use psyche_coordinator::model::LLMTrainingDataLocation;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;

use crate::{
    docker_watcher::DockerWatcherError,
    utils::{ConfigBuilder, SolanaTestClient},
};

/// Check if GPU is available by looking for nvidia-smi or USE_GPU environment variable
fn has_gpu_support() -> bool {
    // Check if USE_GPU environment variable is set
    if std::env::var("USE_GPU").is_ok() {
        return true;
    }

    // Check if nvidia-smi command exists
    Command::new("nvidia-smi")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub const CLIENT_CONTAINER_PREFIX: &str = "test-psyche-test-client";
pub const VALIDATOR_CONTAINER_PREFIX: &str = "test-psyche-solana-test-validator";
pub const NGINX_PROXY_PREFIX: &str = "nginx-proxy";
pub const RUN_OWNER_CONTAINER_PREFIX: &str = "test-psyche-run-owner";

/// 1. Stops docker-compose services
/// 2. Force-removes any remaining test containers by name pattern
pub struct DockerTestCleanup;
impl Drop for DockerTestCleanup {
    fn drop(&mut self) {
        println!("\nStopping containers...");
        let output = Command::new("just")
            .args(["stop_test_infra"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .expect("Failed stop docker compose instances");

        if !output.status.success() {
            panic!("Error: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// FIXME: The config path must be relative to the compose file for now.
pub async fn e2e_testing_setup(
    docker_client: Arc<Docker>,
    init_num_clients: usize,
    min_clients: usize,
) -> DockerTestCleanup {
    e2e_testing_setup_with_datasource(docker_client, init_num_clients, None, min_clients).await
}

pub async fn e2e_testing_setup_with_datasource(
    docker_client: Arc<Docker>,
    init_num_clients: usize,
    data_source: Option<Vec<LLMTrainingDataLocation>>,
    min_clients: usize,
) -> DockerTestCleanup {
    remove_old_client_containers(docker_client).await;

    spawn_psyche_network(init_num_clients, data_source, min_clients).unwrap();

    spawn_ctrl_c_task();

    DockerTestCleanup {}
}

pub async fn e2e_testing_setup_subscription(
    docker_client: Arc<Docker>,
    init_num_clients: usize,
    data_source: Option<Vec<LLMTrainingDataLocation>>,
    min_clients: usize,
) -> DockerTestCleanup {
    remove_old_client_containers(docker_client.clone()).await;

    #[cfg(not(feature = "python"))]
    let builder = ConfigBuilder::new()
        .with_num_clients(init_num_clients)
        .with_data_source(data_source)
        .with_min_clients(min_clients);
    #[cfg(feature = "python")]
    let builder = ConfigBuilder::new()
        .with_num_clients(init_num_clients)
        .with_data_source(data_source)
        .with_min_clients(min_clients)
        .with_architecture("HfAuto")
        .with_batch_size(8 * init_num_clients as u32);

    let config_file_path = builder.build();

    println!("[+] Config file written to: {}", config_file_path.display());
    let mut command = Command::new("just");
    let command = command
        .args([
            "run_test_infra_with_proxies_validator",
            &format!("{init_num_clients}"),
        ])
        .env("CONFIG_PATH", config_file_path.to_str().unwrap())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let output = command
        .output()
        .expect("Failed to spawn docker compose instances");
    if !output.status.success() {
        panic!("Error: {}", String::from_utf8_lossy(&output.stderr));
    }

    println!("\n[+] Docker compose network spawned successfully!");
    println!();

    spawn_ctrl_c_task();

    DockerTestCleanup {}
}

/// Build GPU device request for NVIDIA GPU support
fn build_gpu_device_request() -> DeviceRequest {
    DeviceRequest {
        driver: Some("nvidia".to_string()),
        count: Some(tch::Cuda::device_count()),
        capabilities: Some(vec![vec!["gpu".to_string()]]),
        ..Default::default()
    }
}

/// Build host configuration for container with optional GPU and volume binds
fn build_host_config(network: &str, has_gpu: bool, binds: Option<Vec<String>>) -> HostConfig {
    if has_gpu {
        let device_request = build_gpu_device_request();
        HostConfig {
            device_requests: Some(vec![device_request]),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: Some(network.to_string()),
            binds,
            ..Default::default()
        }
    } else {
        HostConfig {
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: Some(network.to_string()),
            binds,
            ..Default::default()
        }
    }
}

/// Load environment variables from client config file and add GPU capabilities if needed
fn load_client_env_vars(has_gpu: bool, env_file: &str) -> Vec<String> {
    let env_path = format!("../../../config/client/{}", env_file);
    let env_vars: Vec<String> = std::fs::read_to_string(&env_path)
        .expect("Failed to read env file")
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("#") {
                None
            } else {
                Some(line.to_string())
            }
        })
        .collect();

    if has_gpu {
        [env_vars, vec!["NVIDIA_DRIVER_CAPABILITIES=all".to_string()]].concat()
    } else {
        env_vars
    }
}

/// Create and start a Docker container with the given configuration
pub async fn create_and_start_container(
    docker_client: Arc<Docker>,
    container_name: String,
    image: &str,
    env_vars: Vec<String>,
    host_config: HostConfig,
    entrypoint: Option<Vec<&str>>,
) -> Result<String, DockerWatcherError> {
    let options = Some(CreateContainerOptions {
        name: container_name.clone(),
        platform: None,
    });

    let mut config = Config {
        image: Some(image),
        env: Some(env_vars.iter().map(|s| s.as_str()).collect()),
        host_config: Some(host_config),
        ..Default::default()
    };

    if let Some(entrypoint) = entrypoint {
        config.entrypoint = Some(entrypoint);
    }

    docker_client
        .create_container(options, config)
        .await
        .unwrap();

    docker_client
        .start_container::<String>(&container_name, None)
        .await
        .unwrap();

    Ok(container_name)
}

/// Wait for a container to complete, retrieve logs, and clean up
/// Returns the container's exit code or an error
pub async fn wait_for_container_and_cleanup(
    docker_client: Arc<Docker>,
    container_name: &str,
    timeout_secs: u64,
) -> Result<i64, anyhow::Error> {
    // Wait for container to complete with timeout
    println!("Waiting for container to complete...");
    let mut wait_stream =
        docker_client.wait_container(container_name, None::<WaitContainerOptions<String>>);

    let exit_code =
        match tokio::time::timeout(Duration::from_secs(timeout_secs), wait_stream.next()).await {
            Ok(Some(Ok(result))) => {
                println!("Container finished with exit code: {}", result.status_code);
                result.status_code
            }
            Ok(Some(Err(e))) => {
                eprintln!("Error waiting for container: {}", e);
                -1
            }
            Ok(None) => {
                eprintln!("Wait stream ended unexpectedly");
                -1
            }
            Err(_) => {
                eprintln!("Container timed out after {} seconds", timeout_secs);
                -1
            }
        };

    // Get the container logs for debugging
    println!("Retrieving container logs...");
    let mut logs_stream = docker_client.logs(
        container_name,
        Some(LogsOptions::<String> {
            stdout: true,
            stderr: true,
            ..Default::default()
        }),
    );
    while let Some(log) = logs_stream.next().await {
        match log {
            Ok(log_output) => print!("  {}", log_output),
            Err(e) => eprintln!("  Error reading logs: {}", e),
        }
    }

    // Always cleanup the container
    remove_container(docker_client, container_name).await;

    // Return success or error based on exit code
    if exit_code == 0 {
        Ok(exit_code)
    } else {
        Err(anyhow::anyhow!("Container exited with code: {}", exit_code))
    }
}

pub async fn spawn_new_client(docker_client: Arc<Docker>) -> String {
    spawn_new_client_with_options(docker_client.clone(), None, ".env.local").await
}

pub async fn spawn_new_client_with_monitoring(
    docker: Arc<Docker>,
    watcher: &crate::docker_watcher::DockerWatcher,
) -> Result<String, DockerWatcherError> {
    let container_id = spawn_new_client(docker.clone()).await;
    let _monitor = watcher
        .monitor_container(
            &container_id,
            vec![
                psyche_core::IntegrationTestLogMarker::LoadedModel,
                psyche_core::IntegrationTestLogMarker::StateChange,
                psyche_core::IntegrationTestLogMarker::Loss,
            ],
        )
        .unwrap();
    println!("Spawned client {container_id}");
    Ok(container_id)
}

/// Internal helper to spawn a client container with configurable options
pub async fn spawn_new_client_with_options(
    docker_client: Arc<Docker>,
    keypair_path: Option<&str>,
    env_file: &str,
) -> String {
    let has_gpu = has_gpu_support();
    let network_name = "test_psyche-test-network";
    let container_name = get_name_of_new_client_container(docker_client.clone()).await;

    // Build volume binds for keypair and/or config
    let mut binds = Vec::new();
    if let Some(host_keypair_path) = keypair_path {
        let container_keypair_path = "/root/.config/solana/id.json";
        binds.push(format!("{}:{}", host_keypair_path, container_keypair_path));
    }

    let host_config = build_host_config(
        network_name,
        has_gpu,
        if binds.is_empty() { None } else { Some(binds) },
    );

    // Create and start container
    let envs = load_client_env_vars(has_gpu, env_file);
    create_and_start_container(
        docker_client,
        container_name,
        "psyche-solana-test-client",
        envs,
        host_config,
        None,
    )
    .await
    .expect("Failed to create and start client container")
}

pub async fn get_container_names(docker_client: Arc<Docker>) -> (Vec<String>, Vec<String>) {
    let containers = get_client_containers_only(docker_client).await;

    let mut running_containers = Vec::new();
    let mut all_container_names = Vec::new();

    for cont in containers {
        if let Some(name) = container_name(&cont) {
            let name_owned = name.to_string();
            all_container_names.push(name_owned.clone());

            if cont
                .state
                .as_deref()
                .is_some_and(|state| state.eq_ignore_ascii_case("running"))
            {
                running_containers.push(name_owned);
            }
        }
    }

    (all_container_names, running_containers)
}

// Updated spawn function
pub fn spawn_psyche_network(
    init_num_clients: usize,
    data_source: Option<Vec<LLMTrainingDataLocation>>,
    min_clients: usize,
) -> Result<(), DockerWatcherError> {
    #[cfg(not(feature = "python"))]
    let builder = ConfigBuilder::new()
        .with_num_clients(init_num_clients)
        .with_data_source(data_source)
        .with_min_clients(min_clients);
    #[cfg(feature = "python")]
    let builder = ConfigBuilder::new()
        .with_num_clients(init_num_clients)
        .with_data_source(data_source)
        .with_min_clients(min_clients)
        .with_architecture("HfAuto")
        .with_batch_size(8 * init_num_clients as u32);

    let config_file_path = builder.build();

    println!("[+] Config file written to: {}", config_file_path.display());

    let mut command = Command::new("just");
    let output = command
        .args(["run_test_infra", &format!("{init_num_clients}")])
        .env("CONFIG_PATH", config_file_path.to_str().unwrap())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .expect("Failed to spawn docker compose instances");

    if !output.status.success() {
        panic!("Error: {}", String::from_utf8_lossy(&output.stderr));
    }

    println!("\n[+] Docker compose network spawned successfully!");
    println!();
    Ok(())
}

pub fn spawn_ctrl_c_task() {
    tokio::spawn(async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        println!("\nCtrl+C received. Stopping containers...");
        let output = Command::new("just")
            .args(["stop_test_infra"])
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .expect("Failed stop docker compose instances");

        if !output.status.success() {
            panic!("Error: {}", String::from_utf8_lossy(&output.stderr));
        }
        std::process::exit(0);
    });
}

/// Extract the container name from a ContainerSummary, stripping the Docker "/" prefix
fn container_name(cont: &ContainerSummary) -> Option<&str> {
    cont.names
        .as_ref()?
        .first()
        .map(|s| s.trim_start_matches('/'))
}

/// List containers matching given prefixes
async fn list_containers_by_prefix(
    docker_client: Arc<Docker>,
    prefixes: &[&str],
) -> Vec<ContainerSummary> {
    let all_containers = docker_client
        .list_containers::<String>(Some(ListContainersOptions {
            all: true, // Include stopped containers as well
            ..Default::default()
        }))
        .await
        .unwrap();

    // Filter containers by prefixes
    all_containers
        .into_iter()
        .filter(|cont| {
            container_name(cont)
                .map(|name| prefixes.iter().any(|prefix| name.starts_with(prefix)))
                .unwrap_or(false)
        })
        .collect()
}

/// Get ONLY client containers (excludes nginx proxies)
/// Used for counting/naming new client containers
async fn get_client_containers_only(docker_client: Arc<Docker>) -> Vec<ContainerSummary> {
    list_containers_by_prefix(docker_client, &[CLIENT_CONTAINER_PREFIX]).await
}

/// Get all test infrastructure containers (clients + nginx proxies + run owners)
/// Used for cleanup operations
async fn get_test_containers(docker_client: Arc<Docker>) -> Vec<ContainerSummary> {
    list_containers_by_prefix(
        docker_client,
        &[
            CLIENT_CONTAINER_PREFIX,
            NGINX_PROXY_PREFIX,
            RUN_OWNER_CONTAINER_PREFIX,
        ],
    )
    .await
}

async fn remove_old_client_containers(docker_client: Arc<Docker>) {
    let client_containers = get_test_containers(docker_client.clone()).await;
    println!(
        "Removing old containers: {:?}",
        client_containers
            .iter()
            .filter_map(|c| container_name(c))
            .collect::<Vec<&str>>()
    );

    for cont in client_containers.iter() {
        if let Some(name) = container_name(cont) {
            remove_container(docker_client.clone(), name).await;
        }
    }
}

pub async fn remove_container(docker_client: Arc<Docker>, container_name: &str) {
    docker_client
        .remove_container(
            container_name,
            Some(RemoveContainerOptions {
                force: true, // Ensure it's removed even if running
                ..Default::default()
            }),
        )
        .await
        .unwrap();
}

async fn get_name_of_new_client_container(docker_client: Arc<Docker>) -> String {
    let client_containers = get_client_containers_only(docker_client.clone()).await;
    format!("{CLIENT_CONTAINER_PREFIX}-{}", client_containers.len() + 1)
}

pub async fn kill_all_clients(docker: &Docker, signal: &str) {
    let options = Some(KillContainerOptions { signal });
    let (_, running_containers) = get_container_names(docker.clone().into()).await;

    for container in running_containers {
        println!("Killing container {container}");
        docker
            .kill_container(&container, options.clone())
            .await
            .unwrap();
    }

    // Small delay to ensure containers terminate
    tokio::time::sleep(Duration::from_secs(2)).await;
}

/// Pause the run and verify it reaches Paused state
pub async fn pause_and_verify(docker: Arc<Docker>, run_id: &str, solana_client: &SolanaTestClient) {
    use psyche_coordinator::RunState;

    println!("Pausing the run...");
    let result: anyhow::Result<()> = SolanaTestClient::set_paused(docker, run_id, true).await;
    result.expect("Failed to pause run");

    tokio::time::sleep(Duration::from_secs(5)).await;

    let coordinator_state = solana_client.get_run_state().await;
    println!("Coordinator state after pause: {coordinator_state}");

    if coordinator_state != RunState::Paused {
        println!("Waiting for {coordinator_state} → Paused transition...");
        assert!(
            solana_client.wait_for_run_state(RunState::Paused, 30).await,
            "Run should transition to Paused"
        );
    }
    println!("Run successfully paused!");
}

/// Resumes a paused run
pub async fn resume_run(docker: Arc<Docker>, run_id: &str) {
    println!("Resuming the run...");
    let result: anyhow::Result<()> = SolanaTestClient::set_paused(docker, run_id, false).await;
    result.expect("Failed to resume run");
    println!("Run resumed!");
}
