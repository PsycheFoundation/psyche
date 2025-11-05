use bollard::{
    Docker,
    container::{
        Config, CreateContainerOptions, KillContainerOptions, ListContainersOptions,
        RemoveContainerOptions,
    },
    models::DeviceRequest,
    secret::{ContainerSummary, HostConfig},
};
use psyche_client::IntegrationTestLogMarker;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::{path::PathBuf, time::Duration};
use tokio::signal;

use crate::docker_watcher::{DockerWatcher, DockerWatcherError};
use crate::utils::SolanaTestClient;

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
    config: Option<PathBuf>,
) -> DockerTestCleanup {
    remove_old_client_containers(docker_client).await;
    spawn_psyche_network(init_num_clients, config).unwrap();
    spawn_ctrl_c_task();

    DockerTestCleanup {}
}

pub async fn e2e_testing_setup_subscription(
    docker_client: Arc<Docker>,
    init_num_clients: usize,
    config: Option<PathBuf>,
) -> DockerTestCleanup {
    remove_old_client_containers(docker_client.clone()).await;

    let mut command = Command::new("just");
    let command = command
        .args([
            "run_test_infra_with_proxies_validator",
            &format!("{init_num_clients}"),
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(config) = config {
        command.env("CONFIG_PATH", config);
    }

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
        count: Some(1),
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
fn load_client_env_vars(has_gpu: bool) -> Vec<String> {
    let env_vars: Vec<String> = std::fs::read_to_string("../../../config/client/.env.local")
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
async fn create_and_start_container(
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

/// Internal helper to spawn a client container with configurable options
async fn spawn_client_internal(
    docker_client: Arc<Docker>,
    container_name: String,
    keypair_bind: Option<String>,
    custom_entrypoint: Option<Vec<&str>>,
) -> Result<String, DockerWatcherError> {
    let has_gpu = has_gpu_support();
    let network_name = "test_psyche-test-network";

    // Build volume binds if keypair path provided
    let binds = keypair_bind.map(|host_path| {
        let container_keypair_path = "/root/.config/solana/id.json";
        vec![format!("{}:{}", host_path, container_keypair_path)]
    });

    // Build host config with optional GPU and binds
    let host_config = build_host_config(network_name, has_gpu, binds);

    // Load environment variables
    let envs = load_client_env_vars(has_gpu);

    // Create and start container using unified helper
    create_and_start_container(
        docker_client,
        container_name,
        "psyche-solana-test-client-no-python",
        envs,
        host_config,
        custom_entrypoint,
    )
    .await
}

/// Internal helper to spawn a container with keypair and optional config file
/// Used by both spawn_client_with_keypair and spawn_run_owner_with_keypair
async fn spawn_container_with_keypair_and_config(
    docker_client: Arc<Docker>,
    container_name: String,
    host_keypair_path: &str,
    entrypoint: Vec<&str>,
    config_path: Option<&str>,
    additional_env_vars: Vec<String>,
) -> Result<String, DockerWatcherError> {
    let has_gpu = has_gpu_support();
    let network_name = "test_psyche-test-network";
    let container_keypair_path = "/root/.config/solana/id.json";

    // Build volume binds: always include keypair, optionally include config
    let mut binds = vec![format!("{}:{}", host_keypair_path, container_keypair_path)];
    if let Some(config) = config_path {
        let container_config_path = "/usr/local/config.toml";
        binds.push(format!("{}:{}", config, container_config_path));
    }

    let host_config = build_host_config(network_name, has_gpu, Some(binds));

    // Load base environment variables and add any additional ones
    let mut envs = load_client_env_vars(has_gpu);
    envs.extend(additional_env_vars);

    // Create and start container
    create_and_start_container(
        docker_client,
        container_name,
        "psyche-solana-test-client-no-python",
        envs,
        host_config,
        Some(entrypoint),
    )
    .await
}

/// Spawns a new client container with default configuration.
pub async fn spawn_new_client(docker_client: Arc<Docker>) -> Result<String, DockerWatcherError> {
    let new_container_name = get_name_of_new_client_container(docker_client.clone()).await;
    spawn_client_internal(docker_client, new_container_name, None, None).await
}

/// Spawns a new client container with a specific Solana keypair.
/// This allows rejoining with the same identity after disconnecting.
pub async fn spawn_client_with_keypair(
    docker_client: Arc<Docker>,
    host_keypair_path: &str,
) -> Result<String, DockerWatcherError> {
    let new_container_name = get_name_of_new_client_container(docker_client.clone()).await;

    // Use entrypoint script that skips solana-keygen (which would overwrite the mounted keypair)
    let entrypoint = vec!["/bin/client_test_entrypoint_with_keypair.sh"];

    spawn_container_with_keypair_and_config(
        docker_client,
        new_container_name,
        host_keypair_path,
        entrypoint,
        None,       // No config file for regular clients
        Vec::new(), // No additional env vars
    )
    .await
}

/// Spawns a run owner container with a specific Solana keypair.
pub async fn spawn_run_owner_with_keypair(
    docker_client: Arc<Docker>,
    host_keypair_path: &str,
    config_path: &str,
    run_id: &str,
) -> Result<String, DockerWatcherError> {
    let container_name = "test-psyche-run-owner-1".to_string();

    // Use entrypoint script that skips solana-keygen
    let entrypoint = vec!["/bin/run_owner_entrypoint_with_keypair.sh"];

    // Add run-specific environment variable
    let additional_env_vars = vec![format!("RUN_ID={}", run_id)];

    spawn_container_with_keypair_and_config(
        docker_client,
        container_name,
        host_keypair_path,
        entrypoint,
        Some(config_path), // Run owner needs config file
        additional_env_vars,
    )
    .await
}

pub async fn get_container_names(docker_client: Arc<Docker>) -> (Vec<String>, Vec<String>) {
    let containers = get_client_containers_only(docker_client).await;

    let mut running_containers = Vec::new();
    let mut all_container_names = Vec::new();

    for cont in containers {
        if let Some(names) = &cont.names {
            if let Some(name) = names.first() {
                let trimmed_name = name.trim_start_matches('/').to_string();
                all_container_names.push(trimmed_name.clone());

                if cont
                    .state
                    .as_deref()
                    .is_some_and(|state| state.eq_ignore_ascii_case("running"))
                {
                    running_containers.push(trimmed_name);
                }
            }
        }
    }

    (all_container_names, running_containers)
}

pub async fn spawn_new_client_with_monitoring(
    docker: Arc<Docker>,
    watcher: &DockerWatcher,
) -> Result<String, DockerWatcherError> {
    let container_id = spawn_new_client(docker.clone()).await.unwrap();
    let _monitor_client_2 = watcher
        .monitor_container(
            &container_id,
            vec![
                IntegrationTestLogMarker::LoadedModel,
                IntegrationTestLogMarker::StateChange,
                IntegrationTestLogMarker::Loss,
            ],
        )
        .unwrap();
    println!("Spawned client {container_id}");
    Ok(container_id)
}

pub fn spawn_psyche_network(
    init_num_clients: usize,
    config: Option<PathBuf>,
) -> Result<(), DockerWatcherError> {
    let mut command = Command::new("just");
    let command = command
        .args(["run_test_infra", &format!("{init_num_clients}")])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(config) = config {
        command.env("CONFIG_PATH", config);
    }

    let output = command
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
            cont.names
                .as_ref() // Get Option<&Vec<String>>
                .and_then(|names| names.first()) // Get Option<&String> (first name)
                .map(|name| {
                    // Docker prefixes names with "/", so strip it
                    let trimmed_name = name.trim_start_matches('/');
                    // Check if container name starts with any of the provided prefixes
                    prefixes
                        .iter()
                        .any(|prefix| trimmed_name.starts_with(prefix))
                })
                .unwrap_or(false) // If any step failed, exclude this container
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

    for cont in client_containers.iter() {
        docker_client
            .remove_container(
                cont.names
                    .as_ref()
                    .unwrap()
                    .first()
                    .unwrap()
                    .trim_start_matches('/'),
                Some(RemoveContainerOptions {
                    force: true, // Ensure it's removed even if running
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
    }
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
pub async fn pause_and_verify(
    docker: Arc<Docker>,
    run_id: &str,
    keypair_path: &str,
    solana_client: &SolanaTestClient,
) {
    use psyche_coordinator::RunState;

    println!("Pausing the run...");
    SolanaTestClient::set_paused(docker, run_id, true, keypair_path)
        .await
        .expect("Failed to pause run");

    tokio::time::sleep(Duration::from_secs(5)).await;

    let coordinator_state = solana_client.get_run_state().await;
    println!("Coordinator state after pause: {coordinator_state}");

    if coordinator_state == RunState::Cooldown {
        println!("Waiting for Cooldown → Paused transition...");
        assert!(
            solana_client.wait_for_run_state(RunState::Paused, 30).await,
            "Run should transition to Paused"
        );
    } else {
        assert_eq!(coordinator_state, RunState::Paused, "Run should be paused");
    }
    println!("Run successfully paused!");
}

/// Resumes a paused run
pub async fn resume_run(docker: Arc<Docker>, owner_keypair_path: &str, run_id: &str) {
    println!("Resuming the run...");
    SolanaTestClient::set_paused(docker, run_id, false, owner_keypair_path)
        .await
        .expect("Failed to resume run");
    println!("Run resumed!");
}
