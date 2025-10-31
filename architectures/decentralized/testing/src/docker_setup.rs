use bollard::{
    Docker,
    container::{
        Config, CreateContainerOptions, KillContainerOptions, ListContainersOptions,
        RemoveContainerOptions,
    },
    models::DeviceRequest,
    secret::{ContainerSummary, HostConfig},
};
use futures_util::StreamExt;
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
    remove_old_client_containers(docker_client).await;
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

pub async fn spawn_new_client(docker_client: Arc<Docker>) -> Result<String, DockerWatcherError> {
    // Set the container name based on the ones that are already running.
    let new_container_name = get_name_of_new_client_container(docker_client.clone()).await;

    // Check if GPU is available
    let has_gpu = has_gpu_support();

    // Setting extra hosts and optionally nvidia request
    let network_name = "test_psyche-test-network";
    let host_config = if has_gpu {
        // Setting nvidia usage parameters
        let device_request = DeviceRequest {
            driver: Some("nvidia".to_string()),
            count: Some(1),
            capabilities: Some(vec![vec!["gpu".to_string()]]),
            ..Default::default()
        };

        HostConfig {
            device_requests: Some(vec![device_request]),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: Some(network_name.to_string()),
            ..Default::default()
        }
    } else {
        HostConfig {
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: Some(network_name.to_string()),
            ..Default::default()
        }
    };

    // Get env vars from config file
    let env_vars = std::fs::read_to_string("../../../config/client/.env.local")
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<String>>();
    let envs = if has_gpu {
        [env_vars, vec!["NVIDIA_DRIVER_CAPABILITIES=all".to_string()]].concat()
    } else {
        env_vars
    };

    let options = Some(CreateContainerOptions {
        name: new_container_name.clone(),
        platform: None,
    });
    let config = Config {
        image: Some("psyche-solana-test-client-no-python"),
        env: Some(envs.iter().map(|s| s.as_str()).collect()),
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

/// Spawns a new client container with a specific Solana keypair
/// This allows rejoining with the same identity after disconnecting
pub async fn spawn_client_with_keypair(
    docker_client: Arc<Docker>,
    host_keypair_path: &str,
) -> Result<String, DockerWatcherError> {
    // Set the container name based on the ones that are already running.
    let new_container_name = get_name_of_new_client_container(docker_client.clone()).await;

    // Check if GPU is available
    let has_gpu = has_gpu_support();

    // Setting extra hosts and optionally nvidia request
    let network_name = "test_psyche-test-network";

    // Create bind mount for the keypair
    let container_keypair_path = "/root/.config/solana/id.json";
    let binds = vec![format!("{}:{}", host_keypair_path, container_keypair_path)];

    let host_config = if has_gpu {
        // Setting nvidia usage parameters
        let device_request = DeviceRequest {
            driver: Some("nvidia".to_string()),
            count: Some(1),
            capabilities: Some(vec![vec!["gpu".to_string()]]),
            ..Default::default()
        };

        HostConfig {
            device_requests: Some(vec![device_request]),
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: Some(network_name.to_string()),
            binds: Some(binds),
            ..Default::default()
        }
    } else {
        HostConfig {
            extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
            network_mode: Some(network_name.to_string()),
            binds: Some(binds),
            ..Default::default()
        }
    };

    // Get env vars from config file
    let env_vars = std::fs::read_to_string("../../../config/client/.env.local")
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<String>>();
    let envs = if has_gpu {
        [env_vars, vec!["NVIDIA_DRIVER_CAPABILITIES=all".to_string()]].concat()
    } else {
        env_vars
    };

    // Override entrypoint to skip solana-keygen (which would overwrite the mounted keypair)
    // We run the same commands as client_test_entrypoint.sh but WITHOUT solana-keygen new
    let entrypoint_cmd = "set -o errexit && \
         solana config set --url \"${RPC}\" && \
         solana airdrop 10 \"$(solana-keygen pubkey)\" && \
         psyche-solana-client train \
             --wallet-private-key-path \"/root/.config/solana/id.json\" \
             --rpc \"${RPC}\" \
             --ws-rpc \"${WS_RPC}\" \
             --run-id \"${RUN_ID}\" \
             --logs \"json\""
        .to_string();
    let entrypoint = vec!["/bin/sh", "-c", &entrypoint_cmd];

    let options = Some(CreateContainerOptions {
        name: new_container_name.clone(),
        platform: None,
    });
    let config = Config {
        image: Some("psyche-solana-test-client-no-python"),
        env: Some(envs.iter().map(|s| s.as_str()).collect()),
        host_config: Some(host_config),
        entrypoint: Some(entrypoint),
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

/// Extract a Solana keypair from a container and save to host filesystem
pub async fn extract_keypair_from_container(
    docker: &Docker,
    container_name: &str,
    output_path: &str,
) -> Result<(), String> {
    use bollard::container::DownloadFromContainerOptions;
    use std::fs;
    use std::io::Write;
    use tar::Archive;

    let keypair_container_path = "/root/.config/solana/id.json";
    let download_options = Some(DownloadFromContainerOptions {
        path: keypair_container_path.to_string(),
    });

    let mut keypair_tar_stream = docker.download_from_container(container_name, download_options);
    let mut keypair_tar_bytes = Vec::new();

    while let Some(chunk) = keypair_tar_stream.next().await {
        keypair_tar_bytes.extend_from_slice(
            &chunk.map_err(|e| format!("Failed to download keypair: {}", e))?[..],
        );
    }

    // Extract from tar archive
    let mut archive = Archive::new(&keypair_tar_bytes[..]);
    let mut keypair_json = String::new();
    if let Some(entry) = archive
        .entries()
        .map_err(|e| format!("Failed to read tar: {}", e))?
        .next()
    {
        let mut entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        use std::io::Read;
        entry
            .read_to_string(&mut keypair_json)
            .map_err(|e| format!("Failed to read keypair: {}", e))?;
    }

    // Save to host filesystem
    let mut file = fs::File::create(output_path)
        .map_err(|e| format!("Failed to create file {}: {}", output_path, e))?;
    file.write_all(keypair_json.as_bytes())
        .map_err(|e| format!("Failed to write keypair: {}", e))?;

    println!(
        "Extracted keypair from {} to {}",
        container_name, output_path
    );
    Ok(())
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
