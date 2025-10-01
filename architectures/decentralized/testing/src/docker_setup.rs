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

pub async fn e2e_testing_setup_three_clients(
    docker_client: Arc<Docker>,
    config: Option<PathBuf>,
) -> DockerTestCleanup {
    remove_old_client_containers(docker_client).await;
    let mut command = Command::new("just");
    let command = command
        .args(["run_test_infra_three_clients"])
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

    // Setting nvidia usage parameters
    let device_request = DeviceRequest {
        driver: Some("nvidia".to_string()),
        count: Some(1),
        capabilities: Some(vec![vec!["gpu".to_string()]]),
        ..Default::default()
    };

    // Setting extra hosts and nvidia request
    let network_name = "test_psyche-test-network";
    let host_config = HostConfig {
        device_requests: Some(vec![device_request]),
        extra_hosts: Some(vec!["host.docker.internal:host-gateway".to_string()]),
        network_mode: Some(network_name.to_string()),
        ..Default::default()
    };

    // Get env vars from config file
    let env_vars = std::fs::read_to_string("../../../config/client/.env.local")
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<String>>();
    let envs = [env_vars, vec!["NVIDIA_DRIVER_CAPABILITIES=all".to_string()]].concat();

    let options = Some(CreateContainerOptions {
        name: new_container_name.clone(),
        platform: None,
    });
    let config = Config {
        image: Some("psyche-solana-test-client"),
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

    // Use CI-specific docker-compose if CI environment variable is set
    let use_ci = std::env::var("CI").is_ok();
    let just_command = if use_ci {
        "run_test_infra_ci"
    } else {
        "run_test_infra"
    };

    let command = command
        .args([just_command, &format!("{init_num_clients}")])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(config) = config {
        command.env("CONFIG_PATH", config);
    }

    let output = command
        .output()
        .expect("Failed to spawn docker compose instances");
    if !output.status.success() {
        // Collect container logs before panicking
        println!("\n=== CONTAINER LOGS ===");

        let containers = [
            "test-psyche-run-owner-1",
            "test-psyche-solana-test-validator-1",
            "test-psyche-test-client-1",
            "test-psyche-test-client-2",
        ];

        for container in &containers {
            println!("\n--- Logs for {} ---", container);
            if let Ok(log_output) = std::process::Command::new("docker")
                .args(["logs", container])
                .output()
            {
                println!("{}", String::from_utf8_lossy(&log_output.stdout));
                if !log_output.stderr.is_empty() {
                    println!("STDERR: {}", String::from_utf8_lossy(&log_output.stderr));
                }
            } else {
                println!("Failed to get logs for {}", container);
            }
        }

        println!("\n=== END CONTAINER LOGS ===\n");
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
