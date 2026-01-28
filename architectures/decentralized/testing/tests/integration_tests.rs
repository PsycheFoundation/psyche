// Integration tests for decentralized Psyche.
//
// GPU Support:
// By default, these tests run without GPU. To enable GPU support:
// 1. Set the USE_GPU environment variable: `export USE_GPU=1`
// 2. Or ensure nvidia-smi is available (GPU will be auto-detected)
// The test infrastructure will automatically use docker-compose.gpu.yml when GPU is available.
use std::{sync::Arc, time::Duration};

use bollard::container::StartContainerOptions;
use bollard::{Docker, container::KillContainerOptions};
use psyche_coordinator::{RunState, model::Checkpoint};
use psyche_core::IntegrationTestLogMarker;
use psyche_decentralized_testing::docker_setup::e2e_testing_setup_subscription;
use psyche_decentralized_testing::test_context::{
    CLIENT_JOIN_WAIT_SECS, DEFAULT_EPOCHS, DEFAULT_RUN_ID, INITIAL_RUN_WAIT_SECS,
    LOSS_IMPROVEMENT_THRESHOLD, STATE_TRANSITION_WAIT_SECS,
};
use psyche_decentralized_testing::{
    CLIENT_CONTAINER_PREFIX, NGINX_PROXY_PREFIX,
    chaos::{ChaosAction, ChaosScheduler},
    docker_setup::{kill_all_clients, monitor_client, spawn_new_client_with_monitoring},
    docker_watcher::{DockerWatcher, Response},
    test_context::{TestContext, create_liveness_ticker},
    utils::SolanaTestClient,
};
use rstest::*;
use serial_test::serial;

/// spawn 1 clients and run for 3 epochs
/// assert client and coordinator state synchronization
/// assert that the loss decreases in each epoch
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_one_clients_three_epochs_run() {
    // Initialize test context with 1 client
    let mut ctx = TestContext::new(1).await;
    ctx.monitor_all_clients(1);

    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;
    let mut live_interval = create_liveness_ticker();

    loop {
        tokio::select! {
            _ = live_interval.tick() => {
                if let Err(e) = ctx.watcher.monitor_clients_health().await {
                    panic!("{}", e);
                }
            }
            response = ctx.watcher.log_rx.recv() => {
                match response {
                    Some(Response::StateChange(timestamp, _client_1, old_state, new_state, _ , _)) => {
                        let _coordinator_state = ctx.solana_client.get_run_state().await;
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

                            assert!(loss < last_epoch_loss * LOSS_IMPROVEMENT_THRESHOLD);
                            last_epoch_loss = loss;
                            if epoch == DEFAULT_EPOCHS {
                                break;
                            }
                        }
                    }
                    _ => {},
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
    // Initialize test ctx with 2 clients
    let mut ctx = TestContext::new(2).await;
    ctx.monitor_all_clients(2);

    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;
    let mut live_interval = create_liveness_ticker();

    loop {
        tokio::select! {
            _ = live_interval.tick() => {
                if let Err(e) = ctx.watcher.monitor_clients_health().await {
                    panic!("{}", e);
                }
            }
            response = ctx.watcher.log_rx.recv() => {
                match response {
                    Some(Response::StateChange(timestamp, _client_1, old_state, new_state, _ , _)) => {
                        let _coordinator_state = ctx.solana_client.get_run_state().await;
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

                            assert!(loss < last_epoch_loss * LOSS_IMPROVEMENT_THRESHOLD);
                            last_epoch_loss = loss;
                            if epoch == DEFAULT_EPOCHS {
                                break;
                            }
                        }
                    }
                    _ => {},
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
    let mut ctx = TestContext::new(1).await;

    println!("Waiting for run to go on with the first client");
    tokio::time::sleep(Duration::from_secs(INITIAL_RUN_WAIT_SECS)).await;

    println!("Adding new clients");
    for _i in 1..=n_new_clients {
        spawn_new_client_with_monitoring(ctx.docker.clone(), &ctx.watcher)
            .await
            .unwrap();
    }

    let mut liveness_check_interval = create_liveness_ticker();
    let mut clients_with_model = 0;

    loop {
        tokio::select! {
           _ = liveness_check_interval.tick() => {
               println!("Waiting for epoch to end");
                if let Err(e) = ctx.watcher.monitor_clients_health().await {
                    panic!("{}", e);
               }
           }
           response = ctx.watcher.log_rx.recv() => {
               match response {
                     Some(Response::Loss(_client, epoch, step, _loss)) => {
                          if epoch == 1 && step > 22 {
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
    let mut ctx = TestContext::new(1).await;

    tokio::time::sleep(Duration::from_secs(INITIAL_RUN_WAIT_SECS)).await;

    // Spawn client
    spawn_new_client_with_monitoring(ctx.docker.clone(), &ctx.watcher)
        .await
        .unwrap();

    // Create Arc for ChaosScheduler - need separate client instance to avoid borrow issues
    let solana_client = Arc::new(SolanaTestClient::new(DEFAULT_RUN_ID.to_string()).await);
    let scheduler = ChaosScheduler::new(ctx.docker.clone(), solana_client.clone());
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

    let mut interval = create_liveness_ticker();
    println!("Waiting for training to start");
    loop {
        tokio::select! {
           _ = interval.tick() => {
               println!("Waiting for first epoch to finish");
               if let Err(e) = ctx.watcher.monitor_clients_health().await {
                   panic!("{}", e);
               }
               let current_epoch = solana_client.get_current_epoch().await;
               if current_epoch > 1 {
                    panic!("Second epoch started and the clients did not get the model");
               }
           }
           response = ctx.watcher.log_rx.recv() => {
               if let Some(Response::LoadedModel(checkpoint)) = response {
                   // assert client and coordinator state synchronization
                   assert!(checkpoint.starts_with("P2P"), "The model should be obtained from P2P");
                   println!("Client got the model with P2P");
                   return;
               }
           }
        }
    }
}

/// creates a run and spawns 3 clients
/// Then we kill a client, and we verify that the other two clients are still alive and
/// two healthchecks have been sent by those alive clients.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn disconnect_client() {
    // Initialize a Solana run with 3 clients
    let mut ctx = TestContext::new(3).await;
    ctx.monitor_all_clients(3);

    let mut seen_health_checks: Vec<u64> = Vec::new();
    let mut untrained_batches: Vec<Vec<u64>> = Vec::new();
    let mut killed_client = false;

    while let Some(response) = ctx.watcher.log_rx.recv().await {
        match response {
            Response::StateChange(_timestamp, client_id, old_state, new_state, epoch, step) => {
                println!(
                    "epoch: {epoch} step: {step} state change client {client_id} - {old_state} => {new_state}"
                );

                if step == 20 {
                    println!("Max number of epochs reached for test");
                    break;
                }

                if old_state == RunState::WaitingForMembers.to_string() {
                    let epoch_clients = ctx.solana_client.get_current_epoch_clients().await;
                    println!(
                        "Starting epoch: {} with {} clients",
                        epoch,
                        epoch_clients.len()
                    );
                }

                if epoch == 0
                    && step == 10
                    && old_state == RunState::RoundTrain.to_string()
                    && !killed_client
                {
                    let epoch_clients = ctx.solana_client.get_current_epoch_clients().await;
                    assert_eq!(epoch_clients.len(), 3);

                    // Kill any client, since all are witnesses
                    ctx.watcher
                        .kill_container(&format!("{CLIENT_CONTAINER_PREFIX}-1"))
                        .await
                        .unwrap();
                    println!("Killed client: {CLIENT_CONTAINER_PREFIX}-1");
                    killed_client = true;
                }

                if killed_client
                    && !seen_health_checks.is_empty()
                    && new_state == RunState::Cooldown.to_string()
                {
                    let epoch_clients = ctx.solana_client.get_current_epoch_clients().await;
                    assert_eq!(
                        epoch_clients.len(),
                        2,
                        "The remaining number of clients is incorrect"
                    );
                    break;
                }
            }

            // track HealthChecks send
            Response::HealthCheck(unhealthy_client_id, _index, current_step) => {
                println!("found unhealthy client: {unhealthy_client_id:?}");

                let clients_ids: Vec<String> = ctx
                    .solana_client
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
    let mut current_epoch = -1;
    let mut ctx = TestContext::new(n_clients).await;
    ctx.monitor_all_clients(n_clients);

    let mut train_reached = false;
    while let Some(response) = ctx.watcher.log_rx.recv().await {
        match response {
            Response::StateChange(_timestamp, client, old_state, new_state, _epoch, _step) => {
                let coordinator_state = ctx.solana_client.get_run_state().await;
                println!("state change client {client} - {old_state}=>{new_state}");

                // Once warmup starts, kill client 2's container
                if new_state == RunState::RoundTrain.to_string() && !train_reached {
                    println!(
                        "Train started, killing container {}...",
                        &format!("{CLIENT_CONTAINER_PREFIX}-2")
                    );

                    let options = Some(KillContainerOptions { signal: "SIGKILL" });
                    ctx.docker
                        .kill_container(&format!("{CLIENT_CONTAINER_PREFIX}-2"), options)
                        .await
                        .unwrap();

                    tokio::time::sleep(Duration::from_secs(STATE_TRANSITION_WAIT_SECS - 3)).await;
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
                    if epoch == DEFAULT_EPOCHS {
                        println!("Epoch {epoch} reached. Stopping");
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    println!("Waiting 5s to see if we are still in WaitingForMembers...");
    tokio::time::sleep(Duration::from_secs(STATE_TRANSITION_WAIT_SECS)).await;
    let coordinator_state = ctx.solana_client.get_run_state().await;
    assert_eq!(coordinator_state, RunState::WaitingForMembers);
    println!("Still in WaitingForMembers after 5 seconds. Success");

    // Test reconnection
    println!("Starting new client...");
    spawn_new_client_with_monitoring(ctx.docker.clone(), &ctx.watcher)
        .await
        .unwrap();

    // Wait for state to change back to Warmup
    assert!(
        ctx.solana_client
            .wait_for_run_state(RunState::Warmup, 30)
            .await,
        "System should have returned to Warmup state after client reconnection"
    );
    println!("Successfully returned to Warmup state after client reconnection");
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_when_all_clients_disconnect_checkpoint_is_hub() {
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;
    let mut ctx = TestContext::new(2).await;
    let mut has_spawned_new_client_yet = false;
    let mut has_checked_p2p_checkpoint = false;
    let mut liveness_check_interval = create_liveness_ticker();
    println!("starting loop");
    loop {
        tokio::select! {
            _ = liveness_check_interval.tick() => {
                // Show number of connected clients and current state of coordinator
                let clients = ctx.solana_client.get_clients().await;
                let current_epoch = ctx.solana_client.get_current_epoch().await;
                let current_step = ctx.solana_client.get_last_step().await;
                println!(
                    "Clients: {}, Current epoch: {}, Current step: {}",
                    clients.len(),
                    current_epoch,
                    current_step
                );

                // Check that after 1 epoch the checkpoint is P2P since we have 2 clients
                if !has_checked_p2p_checkpoint && current_epoch == 1 {
                    let checkpoint = ctx.solana_client.get_checkpoint().await;
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
                    kill_all_clients(&ctx.docker, "SIGKILL").await;

                    // Wait a while before spawning a new client
                    tokio::time::sleep(Duration::from_secs(CLIENT_JOIN_WAIT_SECS)).await;
                    // Spawn a new client, that should get the model with Hub
                    let joined_container_id = spawn_new_client_with_monitoring(ctx.docker.clone(), &ctx.watcher).await.unwrap();
                    println!("Spawned new client {joined_container_id} to test checkpoint change to Hub");
                    // Spawn another because whe have min_clients=2
                    let joined_container_id = spawn_new_client_with_monitoring(ctx.docker.clone(), &ctx.watcher).await.unwrap();
                    println!("Spawned new client {joined_container_id} to test checkpoint change to Hub");
                    has_spawned_new_client_yet = true;

                    continue;
                }

                if has_spawned_new_client_yet {
                    // Get checkpoint and check if it's Hub, in that case end gracefully
                    let checkpoint = ctx.solana_client.get_checkpoint().await;
                    if matches!(checkpoint, Checkpoint::Hub(_)) {
                        println!("Checkpoint is Hub, test succesful");
                        return;
                    } else {
                        println!("Checkpoint is not Hub yet, waiting...");
                    }
                }
            }
            response = ctx.watcher.log_rx.recv() => {
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
                            if epoch == DEFAULT_EPOCHS {
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
async fn test_solana_subscriptions() {
    // Initialize with subscription setup (can't use TestContext since this uses special setup)
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let mut watcher = DockerWatcher::new(docker.clone());
    let _cleanup = e2e_testing_setup_subscription(docker.clone(), 2).await;

    // Monitor the client containers
    let _monitor_client_1 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-1"),
            vec![IntegrationTestLogMarker::StateChange],
        )
        .unwrap();

    let _monitor_client_2 = watcher
        .monitor_container(
            &format!("{CLIENT_CONTAINER_PREFIX}-2"),
            vec![IntegrationTestLogMarker::SolanaSubscription],
        )
        .unwrap();

    let mut live_interval = create_liveness_ticker();
    let mut subscription_events: Vec<(String, String)> = Vec::new();

    loop {
        tokio::select! {
            _ = live_interval.tick() => {
                if let Err(e) = watcher.monitor_clients_health().await {
                    panic!("{}", e);
                }
            }
            response = watcher.log_rx.recv() => {
                match response {
                    Some(Response::StateChange(_timestamp, _client_1, old_state, new_state, epoch , step)) => {
                        if old_state == RunState::WaitingForMembers.to_string() {
                            println!(
                                "Starting epoch: {epoch}",
                            );
                        }

                        // shutdown subscription 1
                        if step == 5 && new_state == RunState::RoundWitness.to_string(){
                            println!("stop container {NGINX_PROXY_PREFIX}-1");

                            docker
                                .stop_container(&format!("{NGINX_PROXY_PREFIX}-1"), None)
                                .await
                                .unwrap()

                        }
                        // resume subscription 1
                        if step == 15 && new_state == RunState::RoundWitness.to_string(){
                            println!("resume container {NGINX_PROXY_PREFIX}-1");
                            docker
                                .start_container(&format!("{NGINX_PROXY_PREFIX}-1"), None::<StartContainerOptions<String>>)
                                .await
                                .unwrap();

                        }

                        // shutdown subscription 2
                        if step == 25 && new_state == RunState::RoundWitness.to_string() {
                            println!("stop container {NGINX_PROXY_PREFIX}-2");
                            docker
                                .stop_container(&format!("{NGINX_PROXY_PREFIX}-2"), None)
                                .await
                                .unwrap()

                        }
                        // resume subscription 2
                        if step == 45 && new_state == RunState::RoundWitness.to_string() {
                            println!("resume container {NGINX_PROXY_PREFIX}-2");

                            docker
                                .start_container(&format!("{NGINX_PROXY_PREFIX}-2"), None::<StartContainerOptions<String>>)
                                .await
                                .unwrap();
                        }

                        // finish test
                        if epoch == DEFAULT_EPOCHS {
                            break
                        }

                    },
                    Some(Response::SolanaSubscription(url, status)) => {
                        println!("Solana subscriptions {url} status: {status}");
                        subscription_events.push((url , status))
                    }
                    _ => {},
                }
            }

        }
    }
    // skip the first 3 events since init subscriptions can vary the order
    subscription_events = subscription_events[3..].into();
    subscription_events.dedup();
    let expected_subscription_events = [
        // init subscriptions
        (
            format!(r#""ws://{NGINX_PROXY_PREFIX}-2:8902/ws/""#),
            "Subscription Up".into(),
        ),
        (
            format!(r#""ws://{NGINX_PROXY_PREFIX}-1:8901/ws/""#),
            "Subscription Up".into(),
        ),
        (
            format!(r#""ws://{NGINX_PROXY_PREFIX}-1:8901/ws/""#),
            "Subscription Up".into(),
        ),
        // proxy 1 shutdown and reconnection
        (
            format!(r#""ws://{NGINX_PROXY_PREFIX}-1:8901/ws/""#),
            "Subscription Down".into(),
        ),
        (
            format!(r#""ws://{NGINX_PROXY_PREFIX}-1:8901/ws/""#),
            "Subscription Up".into(),
        ),
        // proxy 2 shutdown and reconnection
        (
            format!(r#""ws://{NGINX_PROXY_PREFIX}-2:8902/ws/""#),
            "Subscription Down".into(),
        ),
        (
            format!(r#""ws://{NGINX_PROXY_PREFIX}-2:8902/ws/""#),
            "Subscription Up".into(),
        ),
    ];

    assert_eq!(subscription_events, expected_subscription_events[3..]);
    println!("subscription_events: {subscription_events:?}");
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_everybody_leaves_in_warmup() {
    let mut ctx = TestContext::new(1).await;
    tokio::time::sleep(Duration::from_secs(CLIENT_JOIN_WAIT_SECS)).await;

    let client_1_name = format!("{CLIENT_CONTAINER_PREFIX}-1");
    monitor_client(&ctx.watcher, &client_1_name).unwrap();

    while let Some(response) = ctx.watcher.log_rx.recv().await {
        if let Response::StateChange(_timestamp, _client_id, old_state, new_state, ..) = response {
            println!("Changing from {old_state} to {new_state}");

            if old_state == RunState::WaitingForMembers.to_string()
                && new_state == RunState::Warmup.to_string()
            {
                println!("Warmup reached, killing container...");
                ctx.watcher.kill_container(&client_1_name).await.unwrap();
                break;
            }
        }
    }

    println!("Starting new client...");
    spawn_new_client_with_monitoring(ctx.docker.clone(), &ctx.watcher)
        .await
        .unwrap();
    println!("New client started");

    let mut live_interval = create_liveness_ticker();

    loop {
        tokio::select! {
            _ = live_interval.tick() => {
                if let Err(e) = ctx.watcher.monitor_clients_health().await {
                    panic!("{}", e);
                }
            }
            response = ctx.watcher.log_rx.recv() => {
                if let Some(Response::StateChange(_timestamp, _client_id, old_state, new_state, ..)) = response {
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
    }
}

/// Tests that if your only peer disconnects, the new client goes back to fetching the model from Hub and not P2P
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_lost_only_peer_go_back_to_hub_checkpoint() {
    let mut ctx = TestContext::new(1).await;
    ctx.monitor_all_clients(1);

    let mut first_client_killed = false;
    let mut spawned_second_client = false;

    let second_client_id: String = format!("{CLIENT_CONTAINER_PREFIX}-2");
    let mut live_interval = create_liveness_ticker();
    loop {
        tokio::select! {
            _ = live_interval.tick() => { // Second client should never crash
                if !spawned_second_client {
                    continue;
                }
                if let Err(e) = ctx.watcher.monitor_client_health_by_id(&second_client_id).await {
                    panic!("Second client has crashed after first client was killed. Test Failed. {e}");
                }
            }
            response = ctx.watcher.log_rx.recv() => {
                match response {
                    Some(Response::StateChange(_timestamp, client_id, old_state, new_state, _epoch, step)) => {
                        if new_state != RunState::RoundTrain.to_string() && new_state != RunState::RoundWitness.to_string() {
                            println!(
                                "step={step} -- state change for client {client_id}: {old_state} => {new_state}"
                            );
                        }

                        if new_state == RunState::RoundTrain.to_string() && !spawned_second_client {
                            println!("Joining a second client to the run");
                            let _second_client_id = spawn_new_client_with_monitoring(ctx.docker.clone(), &ctx.watcher).await.unwrap();
                            spawned_second_client = true;
                        }

                        // When cooldown is reached and second client is joined, kill the first client
                        if new_state == RunState::Cooldown.to_string() && !first_client_killed && spawned_second_client {
                            println!("Cooldown reached, killing the first client");

                            ctx.watcher
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
