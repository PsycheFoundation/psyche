use bollard::{
    Docker,
    container::{
        Config, CreateContainerOptions, KillContainerOptions, ListContainersOptions,
        RemoveContainerOptions,
    },
    models::DeviceRequest,
    secret::{ContainerSummary, HostConfig},
};
use psyche_core::IntegrationTestLogMarker;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;

use crate::{
    docker_watcher::{DockerWatcher, DockerWatcherError},
    utils::ConfigBuilder,
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
) -> DockerTestCleanup {
    e2e_testing_setup_with_min(docker_client, init_num_clients, init_num_clients, None).await
}

/// Setup with a custom ConfigBuilder for per-test configuration overrides.
pub async fn e2e_testing_setup_with_config(
    docker_client: Arc<Docker>,
    init_num_clients: usize,
    config_builder: ConfigBuilder,
    owner_keypair_path: Option<&Path>,
) -> DockerTestCleanup {
    remove_old_client_containers(docker_client).await;

    spawn_psyche_network_with_config(init_num_clients, config_builder, owner_keypair_path).unwrap();

    spawn_ctrl_c_task();

    DockerTestCleanup {}
}

/// Setup with explicit min_clients value and optional owner keypair path.
/// Use this when you need different values (e.g., init_num_clients=0, min_clients=1)
/// or when you need to provide a specific owner keypair for pause/resume operations.
pub async fn e2e_testing_setup_with_min(
    docker_client: Arc<Docker>,
    init_num_clients: usize,
    min_clients: usize,
    owner_keypair_path: Option<&Path>,
) -> DockerTestCleanup {
    remove_old_client_containers(docker_client).await;

    spawn_psyche_network_with_min(init_num_clients, min_clients, owner_keypair_path).unwrap();

    spawn_ctrl_c_task();

    DockerTestCleanup {}
}

pub async fn e2e_testing_setup_rpc_fallback(
    docker_client: Arc<Docker>,
    init_num_clients: usize,
) -> DockerTestCleanup {
    remove_old_client_containers(docker_client).await;
    #[cfg(not(feature = "python"))]
    let config_file_path = ConfigBuilder::new()
        .with_num_clients(init_num_clients)
        .build();
    #[cfg(feature = "python")]
    let config_file_path = ConfigBuilder::new()
        .with_num_clients(init_num_clients)
        .with_architecture("HfAuto")
        .with_batch_size(8 * init_num_clients as u32)
        .build();

    println!("[+] Config file written to: {}", config_file_path.display());
    let mut command = Command::new("just");
    let command = command
        .args([
            "run_test_infra_with_rpc_fallback_proxies",
            &format!("{init_num_clients}"),
        ])
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

/// Spawn a new client container.
/// If keypair_path is provided, the keypair file will be mounted into the container
/// and used as the client's wallet.
pub async fn spawn_new_client(
    docker_client: Arc<Docker>,
    keypair_path: Option<&Path>,
) -> Result<String, DockerWatcherError> {
    // Set the container name based on the ones that are already running.
    let new_container_name = get_name_of_new_client_container(docker_client.clone()).await;

    // Check if GPU is available
    let has_gpu = has_gpu_support();

    // Setting extra hosts and optionally nvidia request
    let network_name = "test_psyche-test-network";

    // Build volume binds and extra env vars for keypair
    let (binds, extra_env) = if let Some(path) = keypair_path {
        let abs_path = std::fs::canonicalize(path).expect("Failed to canonicalize keypair path");
        (
            Some(vec![format!("{}:/tmp/wallet.json:ro", abs_path.display())]),
            vec!["WALLET_PRIVATE_KEY_PATH=/tmp/wallet.json".to_string()],
        )
    } else {
        (None, vec![])
    };

    let host_config = if has_gpu {
        // Setting nvidia usage parameters
        let device_request = DeviceRequest {
            driver: Some("nvidia".to_string()),
            count: Some(tch::Cuda::device_count()),
            capabilities: Some(vec![vec!["gpu".to_string()]]),
            ..Default::default()
        };

        HostConfig {
            device_requests: Some(vec![device_request]),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: Some(network_name.to_string()),
            binds,
            ..Default::default()
        }
    } else {
        HostConfig {
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: Some(network_name.to_string()),
            binds,
            ..Default::default()
        }
    };

    // Get env vars from config file
    let mut env_vars: Vec<String> = std::fs::read_to_string("../../../config/client/.env.local")
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

    // Add extra env vars for keypair if provided
    env_vars.extend(extra_env);

    let options = Some(CreateContainerOptions {
        name: new_container_name.clone(),
        platform: None,
    });
    let config = Config {
        image: Some("psyche-solana-test-client"),
        env: Some(env_vars.iter().map(|s| s.as_str()).collect()),
        host_config: Some(host_config),
        ..Default::default()
    };
    docker_client
        .create_container(options, config)
        .await
        .unwrap();
    // Start the container
    docker_client
        .start_container::<String>(&new_container_name, None)
        .await
        .unwrap();
    Ok(new_container_name)
}

pub async fn get_container_names(docker_client: Arc<Docker>) -> (Vec<String>, Vec<String>) {
    let all_containers = docker_client
        .list_containers::<String>(Some(ListContainersOptions {
            all: true, // Include stopped containers as well
            ..Default::default()
        }))
        .await
        .unwrap();

    let mut running_containers = Vec::new();
    let mut all_container_names = Vec::new();

    for cont in all_containers {
        if let Some(names) = &cont.names {
            if let Some(name) = names.first() {
                let trimmed_name = name.trim_start_matches('/').to_string();

                if trimmed_name.starts_with(CLIENT_CONTAINER_PREFIX) {
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
    }

    (all_container_names, running_containers)
}

pub async fn spawn_new_client_with_monitoring(
    docker: Arc<Docker>,
    watcher: &DockerWatcher,
) -> Result<String, DockerWatcherError> {
    let container_id = spawn_new_client(docker.clone(), None).await.unwrap();
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

// Updated spawn function
pub fn spawn_psyche_network(init_num_clients: usize) -> Result<(), DockerWatcherError> {
    spawn_psyche_network_with_min(init_num_clients, init_num_clients, None)
}

/// Spawn the psyche network with explicit min_clients and optional owner keypair.
pub fn spawn_psyche_network_with_min(
    init_num_clients: usize,
    min_clients: usize,
    owner_keypair_path: Option<&Path>,
) -> Result<(), DockerWatcherError> {
    #[cfg(not(feature = "python"))]
    let config_file_path = ConfigBuilder::new()
        .with_num_clients(init_num_clients)
        .with_min_clients(min_clients)
        .build();
    #[cfg(feature = "python")]
    let config_file_path = ConfigBuilder::new()
        .with_num_clients(init_num_clients)
        .with_min_clients(min_clients)
        .with_architecture("HfAuto")
        .with_batch_size(8 * std::cmp::max(init_num_clients, 1) as u32)
        .build();

    println!("[+] Config file written to: {}", config_file_path.display());

    let mut command = Command::new("just");
    let mut cmd = command
        .args(["run_test_infra", &format!("{init_num_clients}")])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // If owner keypair path is provided, pass it to the setup script
    if let Some(path) = owner_keypair_path {
        let abs_path =
            std::fs::canonicalize(path).expect("Failed to canonicalize owner keypair path");
        cmd = cmd.env("OWNER_KEYPAIR_PATH", abs_path);
    }

    let output = cmd
        .output()
        .expect("Failed to spawn docker compose instances");

    if !output.status.success() {
        panic!("Error: {}", String::from_utf8_lossy(&output.stderr));
    }

    println!("\n[+] Docker compose network spawned successfully!");
    println!();
    Ok(())
}

/// Spawn the psyche network with a custom ConfigBuilder and optional owner keypair.
pub fn spawn_psyche_network_with_config(
    init_num_clients: usize,
    config_builder: ConfigBuilder,
    owner_keypair_path: Option<&Path>,
) -> Result<(), DockerWatcherError> {
    #[cfg(feature = "python")]
    let config_builder = config_builder
        .with_architecture("HfAuto")
        .with_batch_size(8 * std::cmp::max(init_num_clients, 1) as u32);

    let config_file_path = config_builder.build();

    println!("[+] Config file written to: {}", config_file_path.display());

    let mut command = Command::new("just");
    let mut cmd = command
        .args(["run_test_infra", &format!("{init_num_clients}")])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(path) = owner_keypair_path {
        let abs_path =
            std::fs::canonicalize(path).expect("Failed to canonicalize owner keypair path");
        cmd = cmd.env("OWNER_KEYPAIR_PATH", abs_path);
    }

    let output = cmd
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

async fn get_client_containers(docker_client: Arc<Docker>) -> Vec<ContainerSummary> {
    let mut client_containers = Vec::new();
    let all_containers = docker_client
        .list_containers::<String>(Some(ListContainersOptions {
            all: true, // Include stopped containers as well
            ..Default::default()
        }))
        .await
        .unwrap();

    for cont in all_containers {
        if let Some(names) = &cont.names {
            if let Some(name) = names.first() {
                let trimmed_name = name.trim_start_matches('/').to_string();
                if trimmed_name.starts_with(CLIENT_CONTAINER_PREFIX)
                    || trimmed_name.starts_with(NGINX_PROXY_PREFIX)
                {
                    client_containers.push(cont);
                }
            }
        }
    }
    client_containers
}

async fn remove_old_client_containers(docker_client: Arc<Docker>) {
    let client_containers = get_client_containers(docker_client.clone()).await;

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
    let client_containers = get_client_containers(docker_client.clone()).await;
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
