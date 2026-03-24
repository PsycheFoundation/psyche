// Integration tests for decentralized Psyche.
//
// GPU Support:
// By default, these tests run without GPU. To enable GPU support:
// 1. Set the USE_GPU environment variable: `export USE_GPU=1`
// 2. Or ensure nvidia-smi is available (GPU will be auto-detected)
// The test infrastructure will automatically use docker-compose.gpu.yml when GPU is available.
use std::{path::PathBuf, sync::Arc, time::Duration};

use anchor_client::solana_sdk::signature::{Keypair, Signer};
use bollard::container::{KillContainerOptions, StartContainerOptions};
use bollard::Docker;
use psyche_coordinator::{RunState, model::Checkpoint};
use psyche_decentralized_testing::docker_setup::e2e_testing_setup_rpc_fallback;
use psyche_decentralized_testing::{
    CLIENT_CONTAINER_PREFIX, NGINX_PROXY_PREFIX,
    chaos::{ChaosAction, ChaosScheduler},
    docker_setup::{
        e2e_testing_setup, e2e_testing_setup_with_min, kill_all_clients, spawn_new_client,
    },
    event_reader,
    utils::{SolanaTestClient, write_keypair_to_file},
};
use psyche_event_sourcing::events::SubscriptionStatus;
use rstest::*;
use serial_test::serial;

/// Wait until the coordinator reaches a given epoch, polling via Solana RPC.
async fn wait_for_epoch(
    solana_client: &SolanaTestClient,
    target_epoch: u16,
    timeout_secs: u64,
) -> u16 {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let epoch = solana_client.get_current_epoch().await;
        if epoch >= target_epoch {
            return epoch;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("Timed out waiting for epoch {target_epoch}, currently at {epoch}");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Wait until the coordinator reaches a given step, polling via Solana RPC.
async fn wait_for_step(
    solana_client: &SolanaTestClient,
    target_step: u32,
    timeout_secs: u64,
) -> u32 {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let step = solana_client.get_last_step().await;
        if step >= target_step {
            return step;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("Timed out waiting for step {target_step}, currently at {step}");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Wait until the coordinator reaches a given run state.
async fn wait_for_state(
    solana_client: &SolanaTestClient,
    target_state: RunState,
    timeout_secs: u64,
) -> RunState {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let state = solana_client.get_run_state().await;
        if state == target_state {
            return state;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("Timed out waiting for state {target_state:?}, currently at {state:?}");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Small delay to let the file backend flush events to disk.
async fn flush_events_delay() {
    tokio::time::sleep(Duration::from_secs(2)).await;
}

/// Kill a container by name using the Docker API.
async fn kill_container(docker: &Docker, name: &str) {
    docker
        .kill_container(name, Some(KillContainerOptions { signal: "SIGKILL" }))
        .await
        .unwrap();
}

/// Check that a container is still running (not EXITED/DEAD). Panics if crashed.
async fn assert_container_healthy(docker: &Docker, name: &str) {
    let container = docker.inspect_container(name, None).await.unwrap();
    let state = container.state.unwrap();
    match state.status {
        Some(bollard::secret::ContainerStateStatusEnum::DEAD)
        | Some(bollard::secret::ContainerStateStatusEnum::EXITED) => {
            panic!("Container {name} has crashed (status: {:?})", state.status);
        }
        _ => {}
    }
}

/// spawn 1 clients and run for 3 epochs
/// assert that the loss decreases in each epoch
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_one_clients_three_epochs_run() {
    let run_id = "test".to_string();
    let num_of_epochs_to_run = 3;

    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let _cleanup = e2e_testing_setup(docker.clone(), 1).await;
    let solana_client = SolanaTestClient::new(run_id, None).await;

    // Wait for training to complete target epochs
    wait_for_epoch(&solana_client, num_of_epochs_to_run as u16, 600).await;
    flush_events_delay().await;

    // Read events and assert loss convergence
    let all_events = event_reader::read_all_node_events(&event_reader::events_host_dir());
    assert!(!all_events.is_empty(), "Expected at least one node's events");

    for (node_id, events) in &all_events {
        let epoch_losses = event_reader::first_loss_per_epoch(events);
        println!("Node {node_id} epoch losses: {epoch_losses:?}");
        assert!(
            !epoch_losses.is_empty(),
            "Node {node_id} should have recorded losses"
        );

        // Assert loss doesn't spike (allows 10% increase between epochs)
        for window in epoch_losses.windows(2) {
            let (_epoch_a, loss_a) = window[0];
            let (epoch_b, loss_b) = window[1];
            assert!(
                loss_b < loss_a * 1.1,
                "Loss spiked between epochs: {loss_a} -> {loss_b} at epoch {epoch_b}"
            );
        }
    }
}

/// spawn 2 clients and run for 3 epochs
/// assert that the loss decreases in each epoch
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_two_clients_three_epochs_run() {
    let run_id = "test".to_string();
    let num_of_epochs_to_run = 3;

    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let _cleanup = e2e_testing_setup(docker.clone(), 2).await;
    let solana_client = SolanaTestClient::new(run_id, None).await;

    wait_for_epoch(&solana_client, num_of_epochs_to_run as u16, 600).await;
    flush_events_delay().await;

    let all_events = event_reader::read_all_node_events(&event_reader::events_host_dir());
    assert!(
        all_events.len() >= 2,
        "Expected events from at least 2 nodes"
    );

    for (node_id, events) in &all_events {
        let epoch_losses = event_reader::first_loss_per_epoch(events);
        println!("Node {node_id} epoch losses: {epoch_losses:?}");
        assert!(
            !epoch_losses.is_empty(),
            "Node {node_id} should have recorded losses"
        );

        for window in epoch_losses.windows(2) {
            let (_epoch_a, loss_a) = window[0];
            let (epoch_b, loss_b) = window[1];
            assert!(
                loss_b < loss_a * 1.1,
                "Loss spiked between epochs: {loss_a} -> {loss_b} at epoch {epoch_b}"
            );
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

    // Initialize a Solana run with 1 client
    let _cleanup = e2e_testing_setup(docker.clone(), 1).await;

    println!("Waiting for run to go on with the first client");
    tokio::time::sleep(Duration::from_secs(60)).await;

    println!("Adding new clients");
    for _ in 1..=n_new_clients {
        spawn_new_client(docker.clone(), None).await.unwrap();
    }

    let run_id = "test".to_string();
    let solana_client = SolanaTestClient::new(run_id, None).await;

    // Wait for new clients to load model — poll event files, panic if epoch 2 starts first
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    loop {
        let epoch = solana_client.get_current_epoch().await;
        if epoch >= 2 {
            panic!("Epoch 2 started and the clients may not have gotten the model via P2P");
        }

        flush_events_delay().await;
        let all_events = event_reader::read_all_node_events(&event_reader::events_host_dir());

        let clients_with_p2p_model: Vec<_> = all_events
            .iter()
            .filter(|(_, events)| {
                event_reader::model_load_complete(events)
                    .iter()
                    .any(|mlc| mlc.checkpoint_source.starts_with("P2P"))
            })
            .collect();

        if clients_with_p2p_model.len() >= n_new_clients as usize {
            println!("All {} new clients got the model via P2P", n_new_clients);
            return;
        }

        if tokio::time::Instant::now() > deadline {
            panic!(
                "Timed out: only {}/{} clients got model via P2P",
                clients_with_p2p_model.len(),
                n_new_clients
            );
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_rejoining_client_delay() {
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let _cleanup = e2e_testing_setup(docker.clone(), 1).await;
    let solana_client = Arc::new(SolanaTestClient::new("test".to_string(), None).await);

    tokio::time::sleep(Duration::from_secs(30)).await;

    // Spawn a second client
    spawn_new_client(docker.clone(), None).await.unwrap();

    // Schedule chaos: delay client-1 at step 20
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

    // Wait for model to be loaded by polling events
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    loop {
        let epoch = solana_client.get_current_epoch().await;
        if epoch > 1 {
            panic!("Second epoch started and the client did not get the model");
        }

        flush_events_delay().await;
        let all_events = event_reader::read_all_node_events(&event_reader::events_host_dir());

        let got_p2p: Vec<_> = all_events
            .iter()
            .filter(|(_, events)| {
                event_reader::model_load_complete(events)
                    .iter()
                    .any(|mlc| mlc.checkpoint_source.starts_with("P2P"))
            })
            .collect();

        if !got_p2p.is_empty() {
            println!("Client got the model with P2P");
            return;
        }

        if tokio::time::Instant::now() > deadline {
            panic!("Timed out waiting for P2P model load");
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// creates a run and spawns 3 clients
/// Then we kill a client, and we verify that the other two clients are still alive and
/// two healthchecks have been sent by those alive clients.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn disconnect_client() {
    let run_id = "test".to_string();
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());

    let _cleanup = e2e_testing_setup(docker.clone(), 3).await;
    let solana_client = SolanaTestClient::new(run_id, None).await;

    // Wait for step 2 in epoch 0, then kill client-1
    wait_for_step(&solana_client, 2, 300).await;

    let epoch_clients = solana_client.get_current_epoch_clients().await;
    assert_eq!(epoch_clients.len(), 3);

    kill_container(&docker, &format!("{CLIENT_CONTAINER_PREFIX}-1")).await;
    println!("Killed client: {CLIENT_CONTAINER_PREFIX}-1");

    // Wait for the epoch to end (coordinator goes back to WaitingForMembers)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    loop {
        let state = solana_client.get_run_state().await;
        let epoch = solana_client.get_current_epoch().await;
        if epoch > 0 && state == RunState::WaitingForMembers {
            println!("Epoch ended after killing client");
            break;
        }
        let step = solana_client.get_last_step().await;
        if step >= 20 {
            println!("Max step reached for test");
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("Timed out waiting for epoch to end after kill");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Verify at most 2 clients remain after the kill
    let epoch_clients = solana_client.get_current_epoch_clients().await;
    assert!(
        epoch_clients.len() <= 2,
        "Expected at most 2 clients after kill, got {}",
        epoch_clients.len()
    );

    flush_events_delay().await;

    // Post-hoc: read events from surviving clients and check health checks + untrained batches
    let all_events = event_reader::read_all_node_events(&event_reader::events_host_dir());

    let mut total_health_checks = 0;
    let mut total_untrained_batches = 0;

    // Collect known client IDs from Solana
    let clients_ids: Vec<String> = solana_client
        .get_clients()
        .await
        .iter()
        .map(|client| client.id.to_string())
        .collect();

    for (node_id, events) in &all_events {
        let hc = event_reader::health_checks(events);
        let ub = event_reader::untrained_batches(events);
        println!(
            "Node {node_id}: {} health checks, {} untrained batch warnings",
            hc.len(),
            ub.len()
        );

        // Verify each health-checked client is in the Solana client list
        for check in &hc {
            assert!(
                clients_ids.contains(&check.client_id),
                "Health-checked client {} not in Solana client list",
                check.client_id
            );
        }

        total_health_checks += hc.len();
        total_untrained_batches += ub.len();
    }

    assert_eq!(
        total_health_checks, 2,
        "Two healthchecks should have been sent (one per surviving client)"
    );

    assert!(
        total_untrained_batches <= 3,
        "Num of untrained batches should be less than 4, got {total_untrained_batches}"
    );
}

// Drop a client below the minimum required, go to WaitingForMembers
// Reconnect a client and then go back to warmup
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn drop_a_client_waitingformembers_then_reconnect() {
    let n_clients = 2;
    let run_id = "test".to_string();
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());

    let _cleanup =
        e2e_testing_setup_with_min(docker.clone(), n_clients, n_clients, None, Some(30)).await;

    let solana_client = SolanaTestClient::new(run_id, None).await;

    // Wait for coordinator to reach WaitingForMembers (both clients joined)
    wait_for_state(&solana_client, RunState::WaitingForMembers, 120).await;

    // Kill client-2
    println!("Both clients in WaitingForMembers. Killing container {CLIENT_CONTAINER_PREFIX}-2...");
    kill_container(&docker, &format!("{CLIENT_CONTAINER_PREFIX}-2")).await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Wait for coordinator to settle back to WaitingForMembers after the kill
    // (it may briefly advance to Warmup, detect dead client, then revert)
    tokio::time::sleep(Duration::from_secs(5)).await;
    let coordinator_state = solana_client.get_run_state().await;
    assert_eq!(coordinator_state, RunState::WaitingForMembers);
    println!("Still in WaitingForMembers after kill. Success");

    // Test reconnection
    println!("Starting new client...");
    spawn_new_client(docker.clone(), None).await.unwrap();

    assert!(
        solana_client.wait_for_run_state(RunState::Warmup, 60).await,
        "System should have returned to Warmup state after client reconnection"
    );
    println!("Successfully returned to Warmup state after client reconnection");
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_when_all_clients_disconnect_checkpoint_is_hub() {
    let run_id = "test".to_string();
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());

    let _cleanup = e2e_testing_setup(docker.clone(), 2).await;
    let solana_client = SolanaTestClient::new(run_id, None).await;

    // Wait for epoch 1 and verify P2P checkpoint (retry until it becomes P2P)
    wait_for_epoch(&solana_client, 1, 600).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let checkpoint = solana_client.get_checkpoint().await;
        if matches!(checkpoint, Checkpoint::P2P(_)) {
            println!("Checkpoint was P2P");
            break;
        }
        if tokio::time::Instant::now() > deadline {
            panic!("Checkpoint should be P2P after epoch with 2 clients");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    tokio::time::sleep(Duration::from_secs(10)).await;

    // Kill all clients
    println!("Killing all clients to test checkpoint change to Hub");
    kill_all_clients(&docker, "SIGKILL").await;

    // Wait before spawning new clients
    tokio::time::sleep(Duration::from_secs(20)).await;

    // Spawn new clients (min_clients=2)
    spawn_new_client(docker.clone(), None).await.unwrap();
    spawn_new_client(docker.clone(), None).await.unwrap();
    println!("Spawned 2 new clients");

    // Poll until checkpoint becomes Hub
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        let checkpoint = solana_client.get_checkpoint().await;
        if matches!(checkpoint, Checkpoint::Hub(_)) {
            println!("Checkpoint is Hub, test successful");
            break;
        }
        println!("Checkpoint is not Hub yet, waiting...");
        if tokio::time::Instant::now() > deadline {
            panic!("Timed out waiting for checkpoint to become Hub");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    flush_events_delay().await;

    // Post-hoc: verify loss convergence from the initial training (before clients were killed)
    let all_events = event_reader::read_all_node_events(&event_reader::events_host_dir());
    for (node_id, events) in &all_events {
        let epoch_losses = event_reader::first_loss_per_epoch(events);
        println!("Node {node_id} epoch losses: {epoch_losses:?}");
        for window in epoch_losses.windows(2) {
            let (_, loss_a) = window[0];
            let (epoch_b, loss_b) = window[1];
            assert!(
                loss_b < loss_a,
                "Loss should strictly decrease: {loss_a} -> {loss_b} at epoch {epoch_b}"
            );
        }
    }
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_everybody_leaves_in_warmup() {
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let _cleanup = e2e_testing_setup(docker.clone(), 1).await;

    let run_id = "test".to_string();
    let solana_client = SolanaTestClient::new(run_id, None).await;

    // Wait for Warmup state
    wait_for_state(&solana_client, RunState::Warmup, 120).await;
    println!("Warmup reached, killing container...");

    let client_1_name = format!("{CLIENT_CONTAINER_PREFIX}-1");
    kill_container(&docker, &client_1_name).await;

    // Start new client and verify it can complete an epoch
    println!("Starting new client...");
    spawn_new_client(docker.clone(), None).await.unwrap();
    println!("New client started");

    // Wait for the new client to go through a full epoch (RoundWitness → Cooldown)
    wait_for_state(&solana_client, RunState::Cooldown, 300).await;
    println!("Epoch restarted correctly, finishing test");
}

/// Tests that if your only peer disconnects, the new client goes back to fetching the model from Hub and not P2P
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_lost_only_peer_go_back_to_hub_checkpoint() {
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
    let _cleanup = e2e_testing_setup(docker.clone(), 1).await;

    let run_id = "test".to_string();
    let solana_client = SolanaTestClient::new(run_id, None).await;

    // Wait for training to start
    wait_for_state(&solana_client, RunState::RoundTrain, 120).await;
    println!("Joining a second client to the run");

    let second_client_name = spawn_new_client(docker.clone(), None).await.unwrap();

    // Wait for Cooldown (second client should be joined by then)
    wait_for_state(&solana_client, RunState::Cooldown, 300).await;
    println!("Cooldown reached, killing the first client");

    kill_container(&docker, &format!("{CLIENT_CONTAINER_PREFIX}-1")).await;
    println!("First client killed, waiting for second client to reload model...");

    // Wait for training to resume (second client re-enters warmup then trains)
    wait_for_state(&solana_client, RunState::RoundTrain, 300).await;
    println!("Second client reached RoundTrain after first was killed, checking events");

    // Verify second client is still alive
    assert_container_healthy(&docker, &second_client_name).await;

    flush_events_delay().await;

    // Post-hoc: check that the second client loaded model from Hub (not P2P)
    let all_events = event_reader::read_all_node_events(&event_reader::events_host_dir());

    let mut found_hub_checkpoint = false;
    for (node_id, events) in &all_events {
        let loads = event_reader::model_load_complete(events);
        for load in &loads {
            println!("Node {node_id}: checkpoint_source = {}", load.checkpoint_source);
        }
        // Check if the last model load (after rejoin) is Hub — verify specific Hub repo prefixes
        if let Some(last_load) = loads.last() {
            let src = &last_load.checkpoint_source;
            if src.starts_with("pefontana/") || src.starts_with("emozilla/") {
                println!("Model successfully obtained from Hub for node {node_id}");
                found_hub_checkpoint = true;
            }
        }
    }

    assert!(
        found_hub_checkpoint,
        "Expected at least one client to load model from Hub after peer disconnected"
    );
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_pause_and_resume_run() {
    let run_id = "test".to_string();
    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());

    let owner_keypair = Arc::new(Keypair::new());
    let client_keypair = Arc::new(Keypair::new());

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

    let _cleanup =
        e2e_testing_setup_with_min(docker.clone(), 0, 1, Some(owner_path.as_path()), None).await;

    let solana_client = SolanaTestClient::new(run_id.clone(), Some(owner_keypair.clone())).await;

    // Spawn client
    let container = spawn_new_client(docker.clone(), Some(client_path.as_path()))
        .await
        .unwrap();
    println!("Spawned client: {}", container);

    // Wait for step 5 then pause
    wait_for_step(&solana_client, 5, 300).await;
    println!("Pausing the run...");
    solana_client
        .set_paused(true)
        .await
        .expect("Failed to pause run");
    println!("Run paused!");

    // Wait for Paused state
    wait_for_state(&solana_client, RunState::Paused, 60).await;
    println!("Coordinator is in Paused state. Killing client and resuming...");

    kill_container(&docker, &container).await;

    tokio::time::sleep(Duration::from_secs(2)).await;

    println!("Resuming the run...");
    solana_client
        .set_paused(false)
        .await
        .expect("Failed to resume run");

    tokio::time::sleep(Duration::from_secs(3)).await;

    // Rejoin with same keypair
    println!("Rejoining with same client keypair...");
    let _new_container = spawn_new_client(docker.clone(), Some(client_path.as_path()))
        .await
        .unwrap();
    println!("Rejoined client: {}", _new_container);

    // Wait for a couple of epochs to verify training continues
    wait_for_epoch(&solana_client, 2, 300).await;
    println!("Trained for 2+ epochs after rejoin, checking events...");

    flush_events_delay().await;

    // Post-hoc: verify loss convergence and hub checkpoint after rejoin
    let all_events = event_reader::read_all_node_events(&event_reader::events_host_dir());
    assert!(
        !all_events.is_empty(),
        "Expected events from at least one node"
    );

    // Check that the rejoined client loaded from Hub (not P2P, since all clients were disconnected)
    // The rejoined client uses the same keypair so shares a node_id.
    // If there are multiple loads, the LAST one (after rejoin) should be Hub.
    let mut found_hub_after_rejoin = false;
    for (node_id, events) in &all_events {
        let loads = event_reader::model_load_complete(events);
        for load in &loads {
            println!("Node {node_id}: checkpoint_source = {}", load.checkpoint_source);
        }
        // Only the last model load matters (that's the one after rejoin)
        if loads.len() >= 2 {
            let last_load = loads.last().unwrap();
            assert!(
                !last_load.checkpoint_source.starts_with("P2P"),
                "After pause/resume with all clients disconnected, checkpoint should be Hub, got: {}",
                last_load.checkpoint_source
            );
            found_hub_after_rejoin = true;
        }

        // Check loss doesn't explode and is positive
        let epoch_losses = event_reader::first_loss_per_epoch(events);
        println!("Node {node_id} epoch losses: {epoch_losses:?}");
        for &(epoch, loss) in &epoch_losses {
            assert!(
                loss > 0.0,
                "Loss should be positive at epoch {epoch}, got {loss}"
            );
        }
        for window in epoch_losses.windows(2) {
            let (_, loss_a) = window[0];
            let (_, loss_b) = window[1];
            assert!(
                loss_b < loss_a * 1.25,
                "Loss should not increase significantly: {loss_a} -> {loss_b}"
            );
        }
    }

    assert!(
        found_hub_after_rejoin,
        "After pause/resume with all clients disconnected, checkpoint should be Hub"
    );
    println!("Test successful!");
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_solana_rpc_fallback() {
    let num_of_epochs_to_run: u16 = 3;

    let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());

    let _cleanup = e2e_testing_setup_rpc_fallback(docker.clone(), 2).await;

    let run_id = "test".to_string();
    let solana_client = SolanaTestClient::new(run_id, None).await;

    // Poll coordinator state/step and stop/start proxies at specific steps
    let mut stopped_primary = false;
    let mut resumed_primary = false;
    let mut stopped_backup = false;
    let mut resumed_backup = false;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
    loop {
        let epoch = solana_client.get_current_epoch().await;
        let step = solana_client.get_last_step().await;

        // Stop primary RPC proxy at step 5
        if step >= 5 && !stopped_primary {
            println!("stop container {NGINX_PROXY_PREFIX}-1 (primary RPC)");
            docker
                .stop_container(&format!("{NGINX_PROXY_PREFIX}-1"), None)
                .await
                .unwrap();
            stopped_primary = true;
        }

        // Resume primary RPC proxy at step 8
        if step >= 8 && !resumed_primary {
            println!("resume container {NGINX_PROXY_PREFIX}-1");
            docker
                .start_container(
                    &format!("{NGINX_PROXY_PREFIX}-1"),
                    None::<StartContainerOptions<String>>,
                )
                .await
                .unwrap();
            resumed_primary = true;
        }

        // Stop backup RPC proxy at step 15
        if step >= 15 && !stopped_backup {
            println!("stop container {NGINX_PROXY_PREFIX}-2 (backup RPC)");
            docker
                .stop_container(&format!("{NGINX_PROXY_PREFIX}-2"), None)
                .await
                .unwrap();
            stopped_backup = true;
        }

        // Resume backup RPC proxy at step 18
        if step >= 18 && !resumed_backup {
            println!("resume container {NGINX_PROXY_PREFIX}-2");
            docker
                .start_container(
                    &format!("{NGINX_PROXY_PREFIX}-2"),
                    None::<StartContainerOptions<String>>,
                )
                .await
                .unwrap();
            resumed_backup = true;
        }

        // Finish after target epochs
        if epoch >= num_of_epochs_to_run {
            break;
        }

        if tokio::time::Instant::now() > deadline {
            panic!(
                "Timed out at epoch {epoch} step {step} waiting for epoch {num_of_epochs_to_run}"
            );
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    flush_events_delay().await;

    // Post-hoc: assert RPC fallback and subscription events from event files
    let all_events = event_reader::read_all_node_events(&event_reader::events_host_dir());

    let mut total_rpc_fallbacks = 0;
    let mut seen_fallback_from_primary = false;
    let mut seen_subscription_down = false;
    let mut seen_subscription_up_after_down = false;

    for (node_id, events) in &all_events {
        let fallbacks = event_reader::rpc_fallbacks(events);
        println!("Node {node_id}: {} RPC fallback events", fallbacks.len());
        for fb in &fallbacks {
            total_rpc_fallbacks += 1;
            if fb.failed_rpc_index == 0 {
                seen_fallback_from_primary = true;
            }
        }

        let subs = event_reader::subscription_changes(events);
        let mut node_seen_down = false;
        for sub in &subs {
            println!(
                "Node {node_id}: subscription {} = {:?}",
                sub.url, sub.status
            );
            if sub.status == SubscriptionStatus::Down {
                seen_subscription_down = true;
                node_seen_down = true;
            }
            if sub.status == SubscriptionStatus::Up && node_seen_down {
                seen_subscription_up_after_down = true;
            }
        }
    }

    println!("Total RPC fallback events: {total_rpc_fallbacks}");
    assert!(
        total_rpc_fallbacks > 0,
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
