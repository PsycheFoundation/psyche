// Integration tests for decentralized Psyche.
//
// GPU Support:
// By default, these tests run without GPU. To enable GPU support:
// 1. Set the USE_GPU environment variable: `export USE_GPU=1`
// 2. Or ensure nvidia-smi is available (GPU will be auto-detected)
// The test infrastructure will automatically use docker-compose.gpu.yml when GPU is available.
use std::{path::PathBuf, sync::Arc, time::Duration};

use anchor_client::solana_sdk::signature::{Keypair, Signer};
use bollard::container::StartContainerOptions;
use bollard::{Docker, container::KillContainerOptions};
use psyche_coordinator::{RunState, model::Checkpoint};
use psyche_core::IntegrationTestLogMarker;
use psyche_decentralized_testing::docker_setup::e2e_testing_setup_rpc_fallback;
use psyche_decentralized_testing::{
    CLIENT_CONTAINER_PREFIX, NGINX_PROXY_PREFIX,
    chaos::{ChaosAction, ChaosScheduler},
    docker_setup::{
        e2e_testing_setup, e2e_testing_setup_with_config, e2e_testing_setup_with_min,
        kill_all_clients, spawn_new_client, spawn_new_client_with_monitoring,
    },
    docker_watcher::{DockerWatcher, Response},
    utils::{ConfigBuilder, SolanaTestClient, write_keypair_to_file},
};
use rstest::*;
use serial_test::serial;
use tokio::time;

/// spawn 1 clients and run for 3 epochs
/// assert client and coordinator state synchronization
/// assert that the loss decreases in each epoch
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_one_clients_three_epochs_run() {
    // set test variables
    let run_id = "test".to_string();

    // epochs the test will run
    let num_of_epochs_to_run = 3;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;

    // Initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(docker.clone(), 1).await;

    // Monitor the client container
    let _monitor_client_1 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-1"),
            vec![
                IntegrationTestLogMarker::StateChange,
                IntegrationTestLogMarker::Loss,
            ],
        )
        .unwrap();

    // Initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id, None).await;
    let mut live_interval = time::interval(Duration::from_secs(10));

    loop {
        tokio::select! {
            _ = live_interval.tick() => {
                if let Err(e) = watcher.monitor_clients_health(1).await {
                    panic!("{}", e);
                }
            }
            response = watcher.log_rx.recv() => {
                match response {
                    Some(Response::StateChange(timestamp, _client_1, old_state, new_state, _ , _)) => {
                        let _coordinator_state = solana_client.get_run_state().await;
                        println!(
                            "client: new_state: {new_state}, old_state: {old_state}, timestamp: {timestamp}"
                        );
                    }
                    Some(Response::Loss(client, epoch, step, loss)) => {
                        println!(
                            "client: {client:?}, epoch: {epoch}, step: {step}, Loss: {loss:?}"
                        );
                        // assert that the loss decreases each epoch or at least dont peak
                        if epoch as i64 > current_epoch {
                            current_epoch = epoch as i64;

                            let Some(loss) = loss else {
                                println!("Reached new epoch but loss was NaN");
                                continue;
                            };

                            assert!(loss < last_epoch_loss * 1.1);
                            last_epoch_loss = loss;
                            if epoch == num_of_epochs_to_run {
                                break;
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

/// spawn 2 clients and run for 3 epochs
/// assert client and coordinator state synchronization
/// assert that the loss decreases in each epoch
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_two_clients_three_epochs_run() {
    // set test variables
    let run_id = "test".to_string();

    // epochs the test will run
    let num_of_epochs_to_run = 3;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;

    // Initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Initialize a Solana run with 2 clients
    let _cleanup = e2e_testing_setup(docker.clone(), 2).await;

    // Monitor the client container
    let _monitor_client_1 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-1"),
            vec![
                IntegrationTestLogMarker::StateChange,
                IntegrationTestLogMarker::Loss,
            ],
        )
        .unwrap();

    let _monitor_client_2 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-2"),
            vec![
                IntegrationTestLogMarker::StateChange,
                IntegrationTestLogMarker::Loss,
            ],
        )
        .unwrap();

    // Initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id, None).await;
    let mut live_interval = time::interval(Duration::from_secs(10));

    loop {
        tokio::select! {
            _ = live_interval.tick() => {
                if let Err(e) = watcher.monitor_clients_health(2).await {
                    panic!("{}", e);
                }
            }
            response = watcher.log_rx.recv() => {
                match response {
                    Some(Response::StateChange(timestamp, _client_1, old_state, new_state, _ , _)) => {
                        let _coordinator_state = solana_client.get_run_state().await;
                        println!(
                            "client: new_state: {new_state}, old_state: {old_state}, timestamp: {timestamp}"
                        );
                    }
                    Some(Response::Loss(client, epoch, step, loss)) => {
                        println!(
                            "client: {client:?}, epoch: {epoch}, step: {step}, Loss: {loss:?}"
                        );
                        // assert that the loss decreases each epoch
                        if epoch as i64 > current_epoch {
                            current_epoch = epoch as i64;

                            let Some(loss) = loss else {
                                println!("Reached new epoch but loss was NaN");
                                continue;
                            };

                            assert!(loss < last_epoch_loss * 1.1);
                            last_epoch_loss = loss;
                            if epoch == num_of_epochs_to_run {
                                break;
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

// Test p2p model sharing process
#[rstest]
#[trace]
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_client_join_and_get_model_p2p(#[values(1, 2)] n_new_clients: u8) {
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(docker.clone(), 1).await;

    println!("Waiting for run to go on with the first client");
    tokio::time::sleep(Duration::from_secs(60)).await;

    println!("Adding new clients");
    for i in 1..=n_new_clients {
        spawn_new_client(docker.clone(), None).await.unwrap();
        let _monitor_client = watcher
            .monitor_container(
                &format!("{CLIENT_CONTAINER_PREFIX}-{}", i + 1),
                vec![
                    IntegrationTestLogMarker::LoadedModel,
                    IntegrationTestLogMarker::Error,
                    IntegrationTestLogMarker::Loss,
                ],
            )
            .unwrap();
        // When spawning multiple clients, stagger them to avoid P2P download
        // contention. This gives the previous client time to download the model
        // and become an additional peer source for subsequent clients.
        if i < n_new_clients {
            println!("Waiting before spawning next client to reduce P2P contention");
            tokio::time::sleep(Duration::from_secs(90)).await;
        }
    }

    let mut liveness_check_interval = time::interval(Duration::from_secs(10));
    let mut clients_with_model = 0;

    loop {
        tokio::select! {
           _ = liveness_check_interval.tick() => {
               println!("Waiting for epoch to end");
                if let Err(e) = watcher.monitor_clients_health(n_new_clients + 1).await {
                    panic!("{}", e);
               }
           }
           response = watcher.log_rx.recv() => {
               match response {
                     Some(Response::Loss(_client, epoch, step, _loss)) => {
                          if epoch >= 2 {
                               panic!("Second epoch started and the clients did not get the model");
                          }
                     }
                     Some(Response::LoadedModel(checkpoint)) => {
                         // assert client and coordinator state synchronization
                         assert!(checkpoint.starts_with("P2P"), "The model should be obtained from P2P");
                         println!("Client got the model with P2P");
                         clients_with_model += 1;
                         if clients_with_model == n_new_clients {
                             println!("All clients got the model with P2P");
                             return;
                         }
                     }
                     _ => {}
               }
           }
        }
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_rejoining_client_delay() {
    // initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // initialize a Solana run with 1 client
    // Use generous warmup so that if client-2 joins during an epoch's warmup,
    // the coordinator waits for it to finish downloading the model via P2P.
    // Client-2 is spawned only AFTER epoch 0 finishes to guarantee it sees P2P.
    // Keep default epoch_time=120 so epoch 0 completes quickly.
    let config = ConfigBuilder::new()
        .with_num_clients(1)
        .with_warmup_time(300);
    let _cleanup = e2e_testing_setup_with_config(docker.clone(), 1, config, None).await;

    let solana_client = Arc::new(SolanaTestClient::new("test".to_string(), None).await);

    // Wait for epoch 0 to complete so checkpoint transitions to P2P.
    // Client-2 must NOT be spawned during epoch 0 or it would see Hub checkpoint.
    // Epoch 0 with 1 client: warmup (~30s) + training (~60s) + cooldown (5s) ≈ ~100-150s
    // warmup_time=300 is a timeout but warmup ends early when client-1 submits.
    println!("Waiting for epoch 0 to complete...");
    let found = solana_client
        .wait_for_run_state(RunState::Cooldown, 600)
        .await;
    assert!(found, "Epoch 0 should reach Cooldown");
    // Wait for cooldown to finish and checkpoint to transition to P2P
    tokio::time::sleep(Duration::from_secs(10)).await;
    let current_epoch = solana_client.get_current_epoch().await;
    println!("Epoch 0 completed, current epoch: {current_epoch}");

    // Spawn client-2 now that checkpoint is P2P
    spawn_new_client(docker.clone(), None).await.unwrap();

    let _monitor_client = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-{}", 2),
            vec![IntegrationTestLogMarker::LoadedModel],
        )
        .unwrap();

    let scheduler = ChaosScheduler::new(docker.clone(), solana_client.clone());
    scheduler
        .schedule_chaos(
            ChaosAction::Delay {
                duration_secs: 30,
                latency_ms: 2000,
                targets: vec![format!("{CLIENT_CONTAINER_PREFIX}-{}", 1)],
            },
            20,
        )
        .await;

    // Use a generous timeout instead of epoch-counting. On CI with 2 vCPUs,
    // the P2P model download can take several minutes while epochs fly by.
    // Client-2 will eventually download the model regardless of epoch count.
    let mut interval = time::interval(Duration::from_secs(10));
    println!("Waiting for client-2 to load model via P2P");
    let result = tokio::time::timeout(Duration::from_secs(600), async {
        loop {
            tokio::select! {
               _ = interval.tick() => {
                   let current_epoch = solana_client.get_current_epoch().await;
                   println!("Waiting for client-2 to load model (epoch: {current_epoch})");
                   // Only check client-1 health. Client-2 may get dropped by the
                   // coordinator if it joins but takes too long downloading the model.
                   // The 600s timeout handles client-2 failures.
                   if let Err(e) = watcher.monitor_clients_health(1).await {
                       panic!("{}", e);
                   }
               }
               response = watcher.log_rx.recv() => {
                   if let Some(Response::LoadedModel(checkpoint)) = response {
                       assert!(checkpoint.starts_with("P2P"), "The model should be obtained from P2P");
                       println!("Client got the model with P2P");
                       return;
                   }
               }
            }
        }
    }).await;
    assert!(
        result.is_ok(),
        "Client-2 did not load model via P2P within 600 seconds"
    );
}

/// creates a run and spawns 3 clients
/// Then we kill a client, and we verify that the other two clients are still alive and
/// two healthchecks have been sent by those alive clients.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn disconnect_client() {
    // set test variables
    let run_id = "test".to_string();

    // initialize a Solana run with 2 client
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Initialize test infrastructure with 0 clients, then spawn them sequentially.
    // On CI (2 vCPUs), starting all 3 simultaneously causes resource starvation
    // where the 3rd client can't complete model loading during warmup.
    // 90s delays between spawns: ensures each client fully loads its model
    //   before the next starts, eliminating CPU contention during loading.
    // warmup_time=300: buffer for the 3rd client (spawned at T=180) to finish
    //   loading after joining the coordinator.
    // max_round_train_time=300: after killing a client, gossip disruption causes
    //   surviving clients to need extra time completing the round.
    //   On 2-vCPU CI runners, rounds can take 180s+ with 3 containers competing.
    // epoch_time=600: enough time for multiple rounds with slow gossip recovery
    let config = ConfigBuilder::new()
        .with_num_clients(3)
        .with_warmup_time(300)
        .with_max_round_train_time(300)
        .with_epoch_time(600);
    let _cleanup = e2e_testing_setup_with_config(docker.clone(), 0, config, None).await;

    // Spawn clients sequentially with delays to reduce CPU contention
    let container_1 = spawn_new_client(docker.clone(), None).await.unwrap();
    let _monitor_client_1 = watcher
        .monitor_container(
            &container_1,
            vec![
                IntegrationTestLogMarker::StateChange,
                IntegrationTestLogMarker::HealthCheck,
                IntegrationTestLogMarker::UntrainedBatches,
                IntegrationTestLogMarker::WitnessElected,
                IntegrationTestLogMarker::Loss,
            ],
        )
        .unwrap();
    println!("Spawned client 1: {container_1}");

    tokio::time::sleep(Duration::from_secs(90)).await;

    let container_2 = spawn_new_client(docker.clone(), None).await.unwrap();
    let _monitor_client_2 = watcher
        .monitor_container(
            &container_2,
            vec![
                IntegrationTestLogMarker::StateChange,
                IntegrationTestLogMarker::HealthCheck,
                IntegrationTestLogMarker::UntrainedBatches,
                IntegrationTestLogMarker::WitnessElected,
                IntegrationTestLogMarker::Loss,
            ],
        )
        .unwrap();
    println!("Spawned client 2: {container_2}");

    tokio::time::sleep(Duration::from_secs(90)).await;

    let container_3 = spawn_new_client(docker.clone(), None).await.unwrap();
    let _monitor_client_3 = watcher
        .monitor_container(
            &container_3,
            vec![
                IntegrationTestLogMarker::StateChange,
                IntegrationTestLogMarker::HealthCheck,
                IntegrationTestLogMarker::UntrainedBatches,
            ],
        )
        .unwrap();
    println!("Spawned client 3: {container_3}");

    // initialize solana client to query the coordinator state
    let solana_client = SolanaTestClient::new(run_id, None).await;

    let mut seen_health_checks: Vec<u64> = Vec::new();
    let mut untrained_batches: Vec<Vec<u64>> = Vec::new();
    let mut killed_client = false;
    // Track which client IDs have been confirmed active in Training state.
    // We only kill after all 3 are verified, to prevent killing when a client
    // dropped out during warmup (which would give wrong health check counts).
    let mut clients_confirmed_training: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    while let Some(response) = watcher.log_rx.recv().await {
        match response {
            Response::StateChange(_timestamp, client_id, old_state, new_state, epoch, step) => {
                println!(
                    "epoch: {epoch} step: {step} state change client {client_id} - {old_state} => {new_state}"
                );

                if step == 20 {
                    println!("Max number of epochs reached for test");
                    break;
                }

                // Safety: if we've moved past epoch 0 without killing a client,
                // the test can't proceed (not enough clients for min_clients).
                // Fail fast rather than hanging.
                if epoch > 0 && !killed_client {
                    panic!(
                        "Epoch advanced to {} without kill happening. Only {} clients confirmed training (needed 3). Test infrastructure issue.",
                        epoch,
                        clients_confirmed_training.len()
                    );
                }

                if old_state == RunState::WaitingForMembers.to_string() {
                    let epoch_clients = solana_client.get_current_epoch_clients().await;
                    println!(
                        "Starting epoch: {} with {} clients",
                        epoch,
                        epoch_clients.len()
                    );
                }

                // Track clients that have entered Training state
                if new_state == RunState::RoundTrain.to_string() {
                    clients_confirmed_training.insert(client_id.clone());
                }

                if killed_client
                    && epoch > 0
                    && new_state == RunState::WaitingForMembers.to_string()
                {
                    println!("Epoch ended after killing client, breaking to verify assertions");
                    break;
                }

                if epoch == 0
                    && step >= 2
                    && old_state == RunState::RoundTrain.to_string()
                    && !killed_client
                    && clients_confirmed_training.len() >= 3
                {
                    let epoch_clients = solana_client.get_current_epoch_clients().await;
                    assert_eq!(epoch_clients.len(), 3);

                    // Kill any client, since all are witnesses
                    watcher.kill_container(&container_1).await.unwrap();
                    println!("Killed client: {container_1}");
                    killed_client = true;
                }

                if killed_client
                    && seen_health_checks.len() >= 2
                    && new_state == RunState::Cooldown.to_string()
                {
                    let epoch_clients = solana_client.get_current_epoch_clients().await;
                    assert!(
                        epoch_clients.len() <= 2,
                        "Expected at most 2 clients after kill, got {}",
                        epoch_clients.len()
                    );
                    break;
                }
            }

            // track HealthChecks send
            Response::HealthCheck(unhealthy_client_id, _index, current_step) => {
                println!("found unhealthy client: {unhealthy_client_id:?}");

                let clients_ids: Vec<String> = solana_client
                    .get_clients()
                    .await
                    .iter()
                    .map(|client| client.id.to_string())
                    .collect();
                seen_health_checks.push(current_step);
                assert!(clients_ids.contains(&unhealthy_client_id));
            }

            // track untrained batches
            Response::UntrainedBatches(untrained_batch_ids) => {
                println!("untrained_batch_ids: {untrained_batch_ids:?}");
                untrained_batches.push(untrained_batch_ids);
            }

            _ => {}
        }
    }

    // assert that two healthchecks were sent, by the alive clients
    assert_eq!(
        seen_health_checks.len(),
        2,
        "Two healthchecks should have been sent"
    );

    // check how many batches where lost due to the client shutdown
    // ideally, we should only lose 2 batches (The ones assigned in the step where it didn't train and the ones where it ran the Health Check and gets kicked)
    // see issue: https://github.com/NousResearch/psyche/issues/269
    assert!(
        untrained_batches.len() <= 3,
        "Num of untrained batches should be less than 4"
    );
}

// Drop a client below the minimum required, go to WaitingForMembers
// Reconnect a client and then go back to warmup
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn drop_a_client_waitingformembers_then_reconnect() {
    let n_clients = 2;
    let num_of_epochs_to_run = 3;
    let mut current_epoch = -1;
    let run_id = "test".to_string();
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    let _cleanup = e2e_testing_setup(docker.clone(), 2).await;

    let solana_client = SolanaTestClient::new(run_id, None).await;
    // Monitor clients
    for i in 1..=n_clients {
        let _monitor_client = watcher
            .monitor_container(
                &format!("{CLIENT_CONTAINER_PREFIX}-{i}"),
                vec![
                    IntegrationTestLogMarker::Loss,
                    IntegrationTestLogMarker::StateChange,
                    IntegrationTestLogMarker::LoadedModel,
                    IntegrationTestLogMarker::Error,
                ],
            )
            .unwrap();
    }

    let mut train_reached = false;
    while let Some(response) = watcher.log_rx.recv().await {
        match response {
            Response::StateChange(_timestamp, client, old_state, new_state, _epoch, _step) => {
                let coordinator_state = solana_client.get_run_state().await;
                println!("state change client {client} - {old_state}=>{new_state}");

                // Once warmup starts, kill client 2's container
                if new_state == RunState::RoundTrain.to_string() && !train_reached {
                    println!(
                        "Train started, killing container {}...",
                        &format!("{CLIENT_CONTAINER_PREFIX}-2")
                    );

                    let options = Some(KillContainerOptions { signal: "SIGKILL" });
                    docker
                        .kill_container(&format!("{CLIENT_CONTAINER_PREFIX}-2"), options)
                        .await
                        .unwrap();

                    tokio::time::sleep(Duration::from_secs(2)).await;
                    train_reached = true;
                }

                // After killing client, verify we get stuck in WaitingForMembers
                if train_reached && coordinator_state == RunState::WaitingForMembers {
                    println!("WaitingForMembers seen");
                    break;
                }
            }
            Response::Loss(client, epoch, step, loss) => {
                println!("client: {client:?}, epoch: {epoch}, step: {step}, Loss: {loss:?}");

                if epoch as i64 > current_epoch {
                    current_epoch = epoch as i64;
                    if epoch == num_of_epochs_to_run {
                        println!("Epoch {epoch} reached. Stopping");
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    println!("Waiting 5s to see if we are still in WaitingForMembers...");
    tokio::time::sleep(Duration::from_secs(5)).await;
    let coordinator_state = solana_client.get_run_state().await;
    assert_eq!(coordinator_state, RunState::WaitingForMembers);
    println!("Still in WaitingForMembers after 5 seconds. Success");

    // Test reconnection
    println!("Starting new client...");
    spawn_new_client(docker.clone(), None).await.unwrap();

    // Wait for state to change back to Warmup
    assert!(
        solana_client.wait_for_run_state(RunState::Warmup, 30).await,
        "System should have returned to Warmup state after client reconnection"
    );
    println!("Successfully returned to Warmup state after client reconnection");
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_when_all_clients_disconnect_checkpoint_is_hub() {
    let num_of_epochs_to_run = 3;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;
    let run_id = "test".to_string();
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    let _cleanup = e2e_testing_setup(docker.clone(), 2).await;

    let solana_client = SolanaTestClient::new(run_id, None).await;
    let mut has_spawned_new_client_yet = false;
    let mut has_checked_p2p_checkpoint = false;
    let mut liveness_check_interval = time::interval(Duration::from_secs(10));
    println!("starting loop");
    loop {
        tokio::select! {
            _ = liveness_check_interval.tick() => {
                // Show number of connected clients and current state of coordinator
                let clients = solana_client.get_clients().await;
                let current_epoch = solana_client.get_current_epoch().await;
                let current_step = solana_client.get_last_step().await;
                println!(
                    "Clients: {}, Current epoch: {}, Current step: {}",
                    clients.len(),
                    current_epoch,
                    current_step
                );

                // Check that after 1 epoch the checkpoint is P2P since we have 2 clients
                if !has_checked_p2p_checkpoint && current_epoch == 1 {
                    let checkpoint = solana_client.get_checkpoint().await;
                    // Assert checkpoint is P2P
                    if matches!(checkpoint, Checkpoint::P2P(_)) {
                        println!("Checkpoint was P2P");
                        has_checked_p2p_checkpoint = true;
                    } else {
                        continue;
                    }

                    // Wait 10 seconds and kill everything
                    tokio::time::sleep(Duration::from_secs(10)).await;

                    println!("Killing all clients to test checkpoint change to Hub");
                    kill_all_clients(&docker, "SIGKILL").await;

                    // Wait a while before spawning a new client
                    tokio::time::sleep(Duration::from_secs(20)).await;
                    // Spawn a new client, that should get the model with Hub
                    let joined_container_id = spawn_new_client_with_monitoring(docker.clone(), &watcher).await.unwrap();
                    println!("Spawned new client {joined_container_id} to test checkpoint change to Hub");
                    // Spawn another because whe have min_clients=2
                    let joined_container_id = spawn_new_client_with_monitoring(docker.clone(), &watcher).await.unwrap();
                    println!("Spawned new client {joined_container_id} to test checkpoint change to Hub");
                    has_spawned_new_client_yet = true;

                    continue;
                }

                if has_spawned_new_client_yet {
                    // Get checkpoint and check if it's Hub, in that case end gracefully
                    let checkpoint = solana_client.get_checkpoint().await;
                    if matches!(checkpoint, Checkpoint::Hub(_)) {
                        println!("Checkpoint is Hub, test succesful");
                        return;
                    } else {
                        println!("Checkpoint is not Hub yet, waiting...");
                    }
                }
            }
            response = watcher.log_rx.recv() => {
                match response {
                    Some(Response::LoadedModel(checkpoint)) => {
                        dbg!(&checkpoint);
                    },
                    Some(Response::Loss(client, epoch, step, loss)) => {
                        println!(
                            "client: {client:?}, epoch: {epoch}, step: {step}, Loss: {loss:?}"
                        );
                        if epoch as i64 > current_epoch {
                            current_epoch = epoch as i64;

                            let Some(loss) = loss else {
                                println!("Reached new epoch but loss was NaN");
                                continue;
                            };

                            assert!(loss < last_epoch_loss);
                            last_epoch_loss = loss;
                            if epoch == num_of_epochs_to_run {
                                println!("Epoch {epoch} reached. Stopping");
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_everybody_leaves_in_warmup() {
    // set test variables
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());

    // initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(docker.clone(), 1).await;
    tokio::time::sleep(Duration::from_secs(20)).await;

    // initialize DockerWatcher
    let mut watcher = DockerWatcher::new(docker.clone());
    let client_1_name = format!("{CLIENT_CONTAINER_PREFIX}-1");

    watcher
        .monitor_container(&client_1_name, vec![IntegrationTestLogMarker::StateChange])
        .unwrap();

    while let Some(response) = watcher.log_rx.recv().await {
        if let Response::StateChange(_timestamp, _client_id, old_state, new_state, ..) = response {
            println!("Changing from {old_state} to {new_state}");

            if old_state == RunState::WaitingForMembers.to_string()
                && new_state == RunState::Warmup.to_string()
            {
                println!("Warmup reached, killing container...");
                watcher.kill_container(&client_1_name).await.unwrap();
                break;
            }
        }
    }

    println!("Starting new client...");
    spawn_new_client(docker.clone(), None).await.unwrap();
    println!("New client started");

    let client_2_name = format!("{CLIENT_CONTAINER_PREFIX}-2");
    watcher
        .monitor_container(&client_2_name, vec![IntegrationTestLogMarker::StateChange])
        .unwrap();

    while let Some(response) = watcher.log_rx.recv().await {
        if let Response::StateChange(_timestamp, _client_id, old_state, new_state, ..) = response {
            println!("Changing from {old_state} to {new_state}");

            if old_state == RunState::RoundWitness.to_string()
                && new_state == RunState::Cooldown.to_string()
            {
                println!("Epoch restarted correctly, finishing test");
                break;
            }
        }
    }
}

/// Tests that if your only peer disconnects, the new client goes back to fetching the model from Hub and not P2P
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_lost_only_peer_go_back_to_hub_checkpoint() {
    // Initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Initialize a Solana run with 1 client, minimum 1 client
    let _cleanup = e2e_testing_setup(docker.clone(), 1).await;

    // Monitor the original client container
    let _monitor_client_1 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-1"),
            vec![IntegrationTestLogMarker::StateChange],
        )
        .unwrap();

    let mut first_client_killed = false;
    let mut spawned_second_client = false;

    let second_client_id: String = format!("{CLIENT_CONTAINER_PREFIX}-2");
    let mut live_interval = time::interval(Duration::from_secs(10));
    loop {
        tokio::select! {
            _ = live_interval.tick() => { // Second client should never crash
                if !spawned_second_client {
                    continue;
                }
                if let Err(e) = watcher.monitor_client_health_by_id(&second_client_id).await {
                    panic!("Second client has crashed after first client was killed. Test Failed. {e}");
                }
            }
            response = watcher.log_rx.recv() => {
                match response {
                    Some(Response::StateChange(_timestamp, client_id, old_state, new_state, _epoch, step)) => {
                        if new_state != RunState::RoundTrain.to_string() && new_state != RunState::RoundWitness.to_string() {
                            println!(
                                "step={step} -- state change for client {client_id}: {old_state} => {new_state}"
                            );
                        }

                        if new_state == RunState::RoundTrain.to_string() && !spawned_second_client {
                            println!("Joining a second client to the run");
                            let second_client_id = spawn_new_client(docker.clone(), None).await.unwrap();
                            let _monitor_client_2 = watcher
                            .monitor_container(
                                &second_client_id,
                                vec![
                                    IntegrationTestLogMarker::StateChange,
                                    IntegrationTestLogMarker::LoadedModel,
                    IntegrationTestLogMarker::Error,
                                    IntegrationTestLogMarker::Loss,
                                ],
                            )
                            .unwrap();
                            spawned_second_client = true;
                        }

                        // When cooldown is reached and second client is joined, kill the first client
                        if new_state == RunState::Cooldown.to_string() && !first_client_killed && spawned_second_client{
                            println!("Cooldown reached, killing the first client");

                            watcher
                                .kill_container(&format!("{CLIENT_CONTAINER_PREFIX}-1"))
                                .await
                                .unwrap();

                            first_client_killed = true;
                            println!("First client killed, waiting to see if second client continues...");
                        }
                    }
                    Some(Response::LoadedModel(checkpoint)) => {
                        if spawned_second_client && first_client_killed {
                            // Assert checkpoint is Hub
                            assert!(checkpoint.starts_with("pefontana/") || checkpoint.starts_with("emozilla/"), "The model should be obtained from Hub since the other client disconnected");
                            println!("Model succesfuly obtained from Hub");
                            return;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_pause_and_resume_run() {
    let run_id = "test".to_string();
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Generate keypairs at runtime
    let owner_keypair = Arc::new(Keypair::new());
    let client_keypair = Arc::new(Keypair::new());

    // Write keypairs to temp files
    let owner_path = PathBuf::from(format!(
        "/tmp/test-owner-keypair-{}.json",
        std::process::id()
    ));
    let client_path = PathBuf::from(format!(
        "/tmp/test-client-keypair-{}.json",
        std::process::id()
    ));
    write_keypair_to_file(&owner_keypair, &owner_path).expect("Failed to write owner keypair");
    write_keypair_to_file(&client_keypair, &client_path).expect("Failed to write client keypair");

    println!("Generated owner keypair: {}", owner_keypair.pubkey());
    println!("Generated client keypair: {}", client_keypair.pubkey());

    // Setup with min_clients=1 but init_num_clients=0 (we spawn manually)
    // Pass owner keypair to setup script
    let _cleanup =
        e2e_testing_setup_with_min(docker.clone(), 0, 1, Some(owner_path.as_path())).await;

    // Create SolanaTestClient with owner keypair for set_paused
    let solana_client = SolanaTestClient::new(run_id.clone(), Some(owner_keypair.clone())).await;

    // Spawn client with generated keypair
    let container = spawn_new_client(docker.clone(), Some(client_path.as_path()))
        .await
        .unwrap();
    println!("Spawned client: {}", container);

    // Monitor the client container
    let _monitor_client = watcher
        .monitor_container(
            &container,
            vec![
                IntegrationTestLogMarker::StateChange,
                IntegrationTestLogMarker::Loss,
                IntegrationTestLogMarker::LoadedModel,
            ],
        )
        .unwrap();

    let mut paused = false;
    let mut client_killed = false;
    let mut rejoined_client = false;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;
    let num_epochs_after_rejoin = 2;

    println!("Waiting for training to start...");
    loop {
        let response = watcher.log_rx.recv().await;
        match response {
            Some(Response::StateChange(_timestamp, _client, old_state, new_state, epoch, step)) => {
                println!("epoch: {epoch} step: {step} state change: {old_state} => {new_state}");

                // Wait until step 5 before pausing
                if !paused && step >= 5 && new_state == RunState::RoundTrain.to_string() {
                    println!("Pausing the run...");
                    solana_client
                        .set_paused(true)
                        .await
                        .expect("Failed to pause run");
                    paused = true;
                    println!("Run paused! Waiting for Paused state...");
                }

                // Race condition: pause can coincide with epoch end, causing the
                // coordinator to enter Cooldown instead of transitioning directly
                // to Paused. When this happens, wait for cooldown_time to expire
                // and re-trigger ticks via set_paused to advance the coordinator.
                if paused && !client_killed && new_state == RunState::Cooldown.to_string() {
                    println!("Cooldown during pause - re-triggering ticks to advance to Paused...");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    let _ = solana_client.set_paused(true).await;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    let _ = solana_client.set_paused(true).await;
                    println!("Re-triggered set_paused ticks after cooldown");
                }

                // When coordinator enters Paused state, kill client and resume
                if paused && !client_killed && new_state == RunState::Paused.to_string() {
                    println!("Coordinator is in Paused state. Killing client and resuming...");

                    // Kill the old container (may already be stopped if cooldown
                    // race occurred and client exited during the wait)
                    if let Err(e) = watcher.kill_container(&container).await {
                        println!("Container {container} already stopped: {e}. Proceeding.");
                    }
                    client_killed = true;

                    // Wait a moment for cleanup
                    tokio::time::sleep(Duration::from_secs(2)).await;

                    // Resume the run
                    println!("Resuming the run...");
                    solana_client
                        .set_paused(false)
                        .await
                        .expect("Failed to resume run");

                    // Wait a moment before rejoining
                    tokio::time::sleep(Duration::from_secs(3)).await;

                    // Spawn new client with SAME keypair
                    println!("Rejoining with same client keypair...");
                    let new_container =
                        spawn_new_client(docker.clone(), Some(client_path.as_path()))
                            .await
                            .unwrap();
                    println!("Rejoined client: {}", new_container);

                    // Monitor the new client
                    watcher
                        .monitor_container(
                            &new_container,
                            vec![
                                IntegrationTestLogMarker::StateChange,
                                IntegrationTestLogMarker::LoadedModel,
                                IntegrationTestLogMarker::Loss,
                            ],
                        )
                        .expect("Failed to monitor rejoined client");

                    rejoined_client = true;
                }
            }
            Some(Response::Loss(client, epoch, step, loss)) => {
                println!("client: {client:?}, epoch: {epoch}, step: {step}, Loss: {loss:?}");

                if rejoined_client && epoch as i64 > current_epoch {
                    current_epoch = epoch as i64;

                    let Some(loss) = loss else {
                        println!("Reached new epoch but loss was NaN");
                        continue;
                    };

                    assert!(
                        loss < last_epoch_loss * 1.25,
                        "Loss should not increase significantly"
                    );
                    assert!(loss > 0.0);
                    last_epoch_loss = loss;

                    // After rejoining, train for a few more epochs to verify training continues
                    if epoch >= num_epochs_after_rejoin {
                        println!(
                            "Trained for {num_epochs_after_rejoin} epochs after rejoin. Loss continued to decrease. Test successful!"
                        );
                        return;
                    }
                }
            }
            Some(Response::LoadedModel(checkpoint)) => {
                println!("LoadedModel checkpoint: {checkpoint}");

                if rejoined_client {
                    // After rejoin, verify client loads from Hub (not P2P)
                    assert!(
                        !checkpoint.starts_with("P2P"),
                        "After pause/resume with all clients disconnected, checkpoint should be Hub, got: {checkpoint}"
                    );
                    println!("Hub checkpoint verified after rejoin!");
                }
            }
            _ => {}
        }
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_solana_rpc_fallback() {
    // epochs the test will run
    let num_of_epochs_to_run = 3;

    // Initialize DockerWatcher
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());

    // Initialize a Solana run with 2 clients using RPC fallback proxies
    let _cleanup = e2e_testing_setup_rpc_fallback(docker.clone(), 2).await;

    // Monitor client 1 for state changes (to track training progress and trigger proxy stops)
    let _monitor_client_1 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-1"),
            vec![IntegrationTestLogMarker::StateChange],
        )
        .unwrap();

    // Monitor client 2 for RPC fallback and subscription events
    let _monitor_client_2 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-2"),
            vec![
                IntegrationTestLogMarker::RpcFallback,
                IntegrationTestLogMarker::SolanaSubscription,
            ],
        )
        .unwrap();

    let mut live_interval = time::interval(Duration::from_secs(10));
    let mut rpc_fallback_count: u32 = 0;
    let mut seen_fallback_from_primary = false;
    let mut seen_subscription_down = false;
    let mut seen_subscription_up_after_down = false;

    loop {
        tokio::select! {
            _ = live_interval.tick() => {
                if let Err(e) = watcher.monitor_clients_health(2).await {
                    panic!("{}", e);
                }
            }
            response = watcher.log_rx.recv() => {
                match response {
                    Some(Response::StateChange(_timestamp, _client_id, old_state, new_state, epoch, step)) => {
                        if old_state == RunState::WaitingForMembers.to_string() {
                            println!("Starting epoch: {epoch}");
                        }

                        // Stop primary RPC proxy at step 5
                        if step == 5 && new_state == RunState::RoundWitness.to_string() {
                            println!("stop container {NGINX_PROXY_PREFIX}-1 (primary RPC)");
                            docker
                                .stop_container(&format!("{NGINX_PROXY_PREFIX}-1"), None)
                                .await
                                .unwrap();
                        }

                        // Resume primary RPC proxy at step 8
                        if step == 8 && new_state == RunState::RoundWitness.to_string() {
                            println!("resume container {NGINX_PROXY_PREFIX}-1");
                            docker
                                .start_container(&format!("{NGINX_PROXY_PREFIX}-1"), None::<StartContainerOptions<String>>)
                                .await
                                .unwrap();
                        }

                        // Stop backup RPC proxy at step 15
                        if step == 15 && new_state == RunState::RoundWitness.to_string() {
                            println!("stop container {NGINX_PROXY_PREFIX}-2 (backup RPC)");
                            docker
                                .stop_container(&format!("{NGINX_PROXY_PREFIX}-2"), None)
                                .await
                                .unwrap();
                        }

                        // Resume backup RPC proxy at step 18
                        if step == 18 && new_state == RunState::RoundWitness.to_string() {
                            println!("resume container {NGINX_PROXY_PREFIX}-2");
                            docker
                                .start_container(&format!("{NGINX_PROXY_PREFIX}-2"), None::<StartContainerOptions<String>>)
                                .await
                                .unwrap();
                        }

                        // Finish test after target epochs
                        if epoch == num_of_epochs_to_run {
                            break;
                        }
                    },
                    Some(Response::RpcFallback(failed_rpc_index, _error)) => {
                        rpc_fallback_count += 1;
                        if failed_rpc_index == "0" {
                            if !seen_fallback_from_primary {
                                println!("RPC fallback from primary (index 0) detected");
                                seen_fallback_from_primary = true;
                            }
                        } else {
                            println!("RPC fallback: failed_rpc_index={failed_rpc_index}");
                        }
                    }
                    Some(Response::SolanaSubscription(url, status)) => {
                        println!("Solana subscription {url} status: {status}");
                        if status == "Subscription Down" {
                            seen_subscription_down = true;
                        }
                        if status == "Subscription Up" && seen_subscription_down {
                            seen_subscription_up_after_down = true;
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    println!("Total RPC fallback events: {rpc_fallback_count}");
    assert!(
        rpc_fallback_count > 0,
        "Expected at least one RPC fallback event, but none were received"
    );
    assert!(
        seen_fallback_from_primary,
        "Expected a fallback from primary RPC (index 0)"
    );
    assert!(
        seen_subscription_down,
        "Expected at least one subscription down event when proxy was stopped"
    );
    assert!(
        seen_subscription_up_after_down,
        "Expected subscription to recover after proxy was resumed"
    );
}
