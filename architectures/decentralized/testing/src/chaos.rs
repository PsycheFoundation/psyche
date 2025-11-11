use std::{collections::HashMap, sync::Arc};

use bollard::{
    Docker,
    container::{CreateContainerOptions, StartContainerOptions},
    image::{CreateImageOptions, ListImagesOptions},
    secret::HostConfig,
};
use futures_util::StreamExt;

use crate::docker_setup::remove_container;
use crate::utils::SolanaTestClient;

#[derive(Clone, Debug)]
pub enum ChaosAction {
    Pause {
        duration_secs: i64,
        targets: Vec<String>,
    },
    Delay {
        duration_secs: i64,
        latency_ms: i64,
        targets: Vec<String>,
    },
    Kill {
        targets: Vec<String>,
    },
    PacketLoss {
        duration_secs: i64,
        loss_percent: f64,
        correlation: f64,
        targets: Vec<String>,
    },
}

#[derive(Clone)]
pub struct ChaosScheduler {
    docker_client: Arc<Docker>,
    solana_client: Arc<SolanaTestClient>,
}

impl ChaosScheduler {
    pub fn new(docker_client: Arc<Docker>, solana_client: Arc<SolanaTestClient>) -> Self {
        Self {
            docker_client,
            solana_client,
        }
    }

    pub async fn schedule_chaos(self, action: ChaosAction, chaos_step: u64) {
        let (mut command, targets) = match action {
            ChaosAction::Pause {
                duration_secs,
                targets,
            } => {
                let duration = format!("{duration_secs}s");
                (
                    vec!["pause".to_string(), "--duration".to_string(), duration],
                    targets,
                )
            }
            ChaosAction::Delay {
                duration_secs,
                latency_ms,
                targets,
            } => {
                let duration = format!("{duration_secs}s");
                let delay_milis = format!("{latency_ms}");
                (
                    vec![
                        "netem".to_string(),
                        "--duration".to_string(),
                        duration,
                        "delay".to_string(),
                        "--jitter".to_string(),
                        "500".to_string(),
                        "--time".to_string(),
                        delay_milis,
                    ],
                    targets,
                )
            }
            ChaosAction::Kill { targets } => (vec!["kill".to_string()], targets),
            ChaosAction::PacketLoss {
                duration_secs,
                loss_percent,
                correlation,
                targets,
            } => {
                let duration = format!("{duration_secs}s");
                (
                    vec![
                        "netem".to_string(),
                        "--duration".to_string(),
                        duration,
                        "loss".to_string(),
                        "--percent".to_string(),
                        loss_percent.to_string(),
                        "--correlation".to_string(),
                        correlation.to_string(),
                    ],
                    targets,
                )
            }
        };

        if chaos_step == 0 {
            pull_image(self.docker_client.clone()).await;
            create_chaos_action_with_command(
                self.docker_client.clone(),
                targets.clone(),
                &mut command,
            )
            .await;
            println!("Chaos correctly applied for containers: {targets:?}");
        } else {
            tokio::spawn({
                async move {
                    loop {
                        let current_step = self.solana_client.get_last_step().await;
                        if current_step >= chaos_step as u32 {
                            pull_image(self.docker_client.clone()).await;
                            create_chaos_action_with_command(
                                self.docker_client.clone(),
                                targets.clone(),
                                &mut command,
                            )
                            .await;
                            println!(
                                "Chaos correctly applied for containers {targets:?} in step {chaos_step}"
                            );
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            });
        }
    }
}

async fn pull_image(docker_client: Arc<Docker>) {
    let filters = HashMap::from([(
        "reference".to_string(),
        vec![format!("gaiaadm/pumba:latest")],
    )]);

    let options = ListImagesOptions {
        all: false,
        filters,
        ..Default::default()
    };

    if docker_client
        .list_images(Some(options))
        .await
        .unwrap()
        .is_empty()
    {
        println!("Pumba image not found, pulling from registry");
        let create_image_options = CreateImageOptions {
            from_image: "gaiaadm/pumba",
            tag: "latest",
            ..Default::default()
        };

        // Pull the image
        let mut stream = docker_client.create_image(Some(create_image_options), None, None);

        while stream.next().await.is_some() {}
        println!("Image pulled successfully!")
    } else {
        println!("Pumba image found in local registry");
    }
}

async fn create_chaos_action_with_command(
    docker_client: Arc<Docker>,
    targets: Vec<String>,
    command: &mut Vec<String>,
) {
    let container_name = "pumba-chaos";

    let network_name = "test_psyche-test-network";
    let host_config = HostConfig {
        network_mode: Some(network_name.to_string()),
        binds: Some(vec![
            "/var/run/docker.sock:/var/run/docker.sock".to_string(),
        ]),
        ..Default::default()
    };

    // Create the container with the Pumba image
    let create_options = CreateContainerOptions {
        name: container_name,
        ..Default::default()
    };

    remove_container(docker_client.clone(), container_name).await;

    for target in targets.iter() {
        command.push(target.clone());
    }

    let container = docker_client
        .create_container(
            Some(create_options),
            bollard::container::Config {
                image: Some("gaiaadm/pumba:latest"),
                cmd: Some(command.iter().map(|c| c.as_str()).collect()),
                host_config: Some(host_config),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Start the container
    docker_client
        .start_container(&container.id, None::<StartContainerOptions<&str>>)
        .await
        .unwrap();
}
