use std::time::Duration;

use psyche_centralized_testing::{
    COOLDOWN_TIME, MAX_ROUND_TRAIN_TIME, ROUND_WITNESS_TIME,
    client::ClientHandle,
    server::CoordinatorServerHandle,
    test_utils::{
        assert_with_retries, assert_witnesses_healthy_score, spawn_clients,
        spawn_clients_with_training_delay,
    },
};
use psyche_coordinator::{RunState, model::Checkpoint};
use tracing::info;

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn connect_single_node() {
    let init_min_clients = 2;
    let global_batch_size = 4;
    let witness_nodes = 1;
    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handle = ClientHandle::default(server_port, run_id).await;
    let connected_clients = || server_handle.get_pending_clients_len();

    assert_with_retries(connected_clients, 1).await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn connect_multiple_nodes() {
    let number_of_nodes = 10;
    let init_min_clients = 15;
    let global_batch_size = 4;
    let witness_nodes = 1;
    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients(number_of_nodes, server_port, run_id).await;

    let connected_clients = || server_handle.get_pending_clients_len();
    let run_state = || server_handle.get_run_state();

    assert_with_retries(connected_clients, number_of_nodes).await;
    assert_with_retries(run_state, RunState::WaitingForMembers).await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn state_change_waiting_for_members_to_warmup() {
    // Coordinator is initialized with some default values
    let init_min_clients = 2;
    let global_batch_size = 4;
    let witness_nodes = 1;
    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    let run_state = || server_handle.get_run_state();
    let connected_clients = || server_handle.get_clients_len();

    // No clients are connected yet, so run state should be `WaitingForMembers`

    assert_with_retries(connected_clients, 0).await;
    assert_with_retries(run_state, RunState::WaitingForMembers).await;

    // Clients are spawned

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients(init_min_clients as usize, server_port, run_id).await;

    // Clients have connected and now that the initial min clients has been reached, run state
    // changes to `Warmup`

    assert_with_retries(connected_clients, 2).await;
    assert_with_retries(run_state, RunState::Warmup).await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn state_change_shutdown_node_in_warmup() {
    // Coordinator is initialized with some default values
    let init_min_clients = 2;
    let global_batch_size = 4;
    let witness_nodes = 1;
    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    // No clients are connected yet, so run state should be `WaitingForMembers`

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    // Clients are spawned and state changes to `Warmup`

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let [_client_1_task, client_2_task]: [ClientHandle; 2] = spawn_clients(2, server_port, run_id)
        .await
        .try_into()
        .unwrap();

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // One client is killed, and now state returns to `WaitingForMembers` since the
    // minimum for starting the round is not reached
    client_2_task.client_handle.abort();
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;
    assert_with_retries(|| server_handle.get_clients_len(), 1).await;
    assert_with_retries(|| server_handle.get_pending_clients_len(), 1).await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn state_change_waiting_for_members_to_round_train() {
    // Coordinator is initialized with some default values
    let init_min_clients = 2;
    let global_batch_size = 4;
    let witness_nodes = 1;
    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients(2, server_port, run_id).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn state_change_waiting_for_members_to_round_witness() {
    let init_min_clients = 2;
    let global_batch_size = 4;
    let witness_nodes = 1;
    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients(2, server_port, run_id).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;

    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;

    // train time
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;

    assert_with_retries(|| server_handle.get_clients_len(), 2).await;
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn validate_all_clients_participate_in_witness_bloom() {
    // We make sure that the number of clients and the batches per round are the same
    // It is important that the number of clients is not greater than the number of batches per round,
    // since if that is the case, there will be clients that will have no data to train in a given round
    // and they won't appear in the bloom filters, making the test fail
    let init_min_clients = 5;
    let global_batch_size = init_min_clients;
    let witness_nodes = 1;
    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients(init_min_clients as usize, server_port, run_id).await;

    // assert that we start in the round 0
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;
    // witnesses should be empty
    assert!(server_handle.get_rounds().await[0].witnesses.is_empty());

    // Start round 0

    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;

    // witness
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    // get to round 2 (where we have witnesses)
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;
    assert_with_retries(|| server_handle.get_rounds_head(), 2).await;

    // Assert that the witness healthy score of the previous round
    // 1 witness, 5 clients and each one trained 1 round, expected_score = 5
    assert_witnesses_healthy_score(&server_handle, 1, 5).await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn replace_node_and_complete_round() {
    let init_min_clients = 2;
    let global_batch_size = 2;
    let witness_nodes = 1;
    let training_delay = 2;
    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;

    info!("initializing clients...");
    let [client_1_task, _client_2_task] = spawn_clients_with_training_delay(
        init_min_clients as usize,
        server_port,
        run_id,
        training_delay,
    )
    .await
    .try_into()
    .unwrap();

    // assert that we start in the round 0
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;
    // witnesses should be empty
    assert!(server_handle.get_rounds().await[0].witnesses.is_empty());

    info!("waiting for warmup...");

    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // A new client is spawned, but since we are in `Warmup` state, it should wait for `WaitingForMembers`
    // to join the run

    info!("creating third client...");
    let _client_handle_3 =
        ClientHandle::new_with_training_delay(server_port, run_id, training_delay).await;

    // A client is killed and the coordinator state returns to `WaitingForMembers`. Since client 3
    // was pending, the state immediately changes to `Warmup` again
    client_1_task.client_handle.abort();

    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // The network advances normally

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(training_delay)).await;

    // witness
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;
}

#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn finish_epoch() {
    // We initialize the coordinator with the same number of min clients as batches per round.
    // This way, every client will be assigned with only one batch
    let init_min_clients = 2;
    let global_batch_size = 2;
    let witness_nodes = 1;
    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let training_delay = 2;
    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let _client_handles = spawn_clients_with_training_delay(
        init_min_clients as usize,
        server_port,
        run_id,
        training_delay,
    )
    .await;

    // assert that we start in the round 0
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;

    // Witnesses should be empty, since round just started and we haven't trained yet
    assert!(server_handle.get_rounds().await[0].witnesses.is_empty());

    // execute round 0
    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    tokio::time::sleep(Duration::from_secs(training_delay)).await;

    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;
    assert_with_retries(|| server_handle.get_rounds_head(), 2).await;
    assert_with_retries(|| server_handle.get_rounds_head(), 3).await;

    // Cooldown
    assert_with_retries(|| server_handle.get_run_state(), RunState::Cooldown).await;
    tokio::time::sleep(Duration::from_secs(COOLDOWN_TIME)).await;

    assert_with_retries(|| server_handle.get_current_epoch(), 1).await;
}

/// A new client attempts to join the network during the RoundTrain phase.
/// The new client should not participate in the current round
/// and should attempt to join the network in the subsequent round.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn client_join_in_training() {
    // start a normal run with 2 clients
    let init_min_clients = 2;
    let global_batch_size = 2;
    let witness_nodes = 1;

    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let training_delay = 2;
    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;

    let _client_handles = spawn_clients_with_training_delay(
        init_min_clients as usize,
        server_port,
        run_id,
        training_delay,
    )
    .await;

    info!("waiting for init min clients...");
    assert_with_retries(
        || server_handle.get_clients_len(),
        init_min_clients as usize,
    )
    .await;

    // execute round 0
    info!("waiting for round 0...");
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;

    // warmup
    info!("waiting for warmup...");
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // train
    info!("waiting for start of train...");
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;

    // spawn new client
    let [_new_client_handle] =
        spawn_clients_with_training_delay(1, server_port, run_id, training_delay)
            .await
            .try_into()
            .unwrap();

    // assert new client didnt join the round but is ready in pending clients
    info!("waiting for pending clients to contain new client");
    assert_with_retries(|| server_handle.get_pending_clients_len(), 3).await;
    assert_with_retries(|| server_handle.get_clients_len(), 2).await;

    // train
    info!("waiting for witness state...");
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    info!("waiting for next round!");
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;

    // run through the rest of the epoch
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;
    assert_with_retries(|| server_handle.get_rounds_head(), 2).await;
    assert_with_retries(|| server_handle.get_rounds_head(), 3).await;

    // Assert that the witness healthy score of the previous round
    // 1 witness, 2 clients and each one trained 1 batch, expected_score = 2
    assert_witnesses_healthy_score(&server_handle, 1, 2).await;

    // check that the run state evolves naturally to Warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // check that the clients length shows the new joined client
    assert_with_retries(|| server_handle.get_clients_len(), 3).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_node_in_training_and_complete_round() {
    let init_min_clients = 3;
    let global_batch_size = 3;
    let witness_nodes = 2;
    let training_delay = 2;
    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;
    let [client_1_task, _client_2_task, _client_3_task] = spawn_clients_with_training_delay(
        init_min_clients as usize,
        server_port,
        run_id,
        training_delay,
    )
    .await
    .try_into()
    .unwrap();

    // warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // train
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;
    let clients = server_handle.get_clients().await;
    assert_eq!(clients.len(), 3);

    // shutdown node 1.
    // this round's workload should be handled entirely by node 2 and 3.
    client_1_task.client_handle.abort();

    // witness
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundWitness).await;
    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

    // since up nodes < init_min_clients
    // the network should return to Cooldown and the WaitingForMembers
    assert_with_retries(|| server_handle.get_run_state(), RunState::Cooldown).await;

    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    // spawn new client
    let [_new_client_handle] =
        spawn_clients_with_training_delay(1, server_port, run_id, training_delay)
            .await
            .try_into()
            .unwrap();

    // Now that the new client joined, we assert that the run state evolved to Warmup
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // The new clients length now includes the joined client
    assert_with_retries(|| server_handle.get_clients_len(), 3).await;
}

// TODO: fix this up for overlapped, something weird with it at step 2

// #[tokio::test(flavor = "multi_thread")]
// async fn kick_node_that_dont_train() {
//     let init_min_clients = 2;
//     let global_batch_size = 2;
//     // all nodes are witness
//     let witness_nodes = 0;
//     // set witness_quorum = 1, as one node will kicked
//     let witness_quorum = 1;
//     let server_handle = CoordinatorServerHandle::new(
//         init_min_clients,
//         global_batch_size,
//         witness_nodes,
//         witness_quorum,
//     )
//     .await;

//     assert_with_retries(|| server_handle.get_clients_len(), 0).await;
//     assert_with_retries(
//         || server_handle.get_run_state(),
//         RunState::WaitingForMembers,
//     )
//     .await;

//     // spawn two clients
//     // client_1 is a normal client
//     // client_2 will take to much time to train, more than MAX_ROUND_TRAIN_TIME.
//     let training_delay_client_1 = 2;
//     let training_delay_client_2 = MAX_ROUND_TRAIN_TIME * 5;
//     let server_port = server_handle.server_port;
//     let run_id = &server_handle.run_id;
//     let _client_1 =
//         spawn_clients_with_training_delay(1, server_port, run_id, training_delay_client_1).await;
//     let _client_2 =
//         spawn_clients_with_training_delay(1, server_port, run_id, training_delay_client_2).await;

//     // assert that we start in the round 0 and the two clients are present
//     assert_with_retries(|| server_handle.get_rounds_head(), 0).await;
//     assert_with_retries(|| server_handle.get_clients_len(), 2).await;

//     tokio::time::sleep(Duration::from_secs(WARMUP_TIME)).await;
//     tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;
//     tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

//     assert_with_retries(|| server_handle.get_rounds_head(), 1).await;
//     tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;
//     tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

//     assert_with_retries(|| server_handle.get_rounds_head(), 2).await;
//     tokio::time::sleep(Duration::from_secs(MAX_ROUND_TRAIN_TIME)).await;
//     tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;

//     // Since client_1 dont get to train any batch in the round 0, in round 3 it should be kicked out of the network
//     assert_with_retries(|| server_handle.get_rounds_head(), 3).await;
//     assert_with_retries(|| server_handle.get_clients_len(), 1).await;
//     assert_with_retries(|| server_handle.get_pending_clients_len(), 1).await;
// }

/// A new client attempts to joins the network in the middle of a run.
/// In the next warmup state it should request the model via P2P to the other clients.
/// The new client can train a whole epoch with the new obtained model.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn client_join_in_training_and_get_model_using_p2p() {
    // start a normal run with 2 clients
    let init_min_clients = 2;
    let global_batch_size = 3;
    let witness_nodes = 1;

    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let training_delay = 1;
    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;

    let _client_handles = spawn_clients_with_training_delay(
        init_min_clients as usize,
        server_port,
        run_id,
        training_delay,
    )
    .await;

    info!("waiting for init min clients...");
    assert_with_retries(
        || server_handle.get_clients_len(),
        init_min_clients as usize,
    )
    .await;

    // execute round 0
    info!("waiting for round 0...");
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;

    // warmup
    info!("waiting for warmup...");
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // train
    info!("waiting for start of train...");
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;

    // spawn new client
    let [_new_client_handle] =
        spawn_clients_with_training_delay(1, server_port, run_id, training_delay)
            .await
            .try_into()
            .unwrap();

    info!("waiting for round 1...");
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;

    info!("waiting for round 2...");
    assert_with_retries(|| server_handle.get_rounds_head(), 2).await;

    info!("waiting for round 3...");
    assert_with_retries(|| server_handle.get_rounds_head(), 3).await;

    info!("waiting for next epoch!");
    assert_with_retries(|| server_handle.get_current_epoch(), 1).await;

    assert_with_retries(
        || server_handle.get_checkpoint(),
        std::mem::discriminant(&Checkpoint::P2PDummy),
    )
    .await;

    // check that the run state evolves naturally to Warmup where the model gets shared
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    info!("waiting for end of round!");
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;
    assert_with_retries(|| server_handle.get_rounds_head(), 2).await;
    assert_with_retries(|| server_handle.get_rounds_head(), 3).await;

    info!("waiting for next epoch!");
    assert_with_retries(|| server_handle.get_current_epoch(), 2).await;

    // check that the clients length shows the new joined client trained with new p2p shared model
    assert_with_retries(|| server_handle.get_clients_len(), 3).await;
}

/// Two new clients attempt to join the network in the middle of a run.
/// In the next warmup state they should request the model via P2P to the other clients.
/// The clients should request not initialized parameters between each other but they should try with other peer.
/// The new clients can train a whole epoch with the new obtained model.
#[test_log::test(tokio::test(flavor = "multi_thread"))]
async fn two_clients_join_in_training_and_get_model_using_p2p() {
    // start a normal run with 2 clients
    let init_min_clients = 2;
    let global_batch_size = 4;
    let witness_nodes = 1;

    let server_handle =
        CoordinatorServerHandle::new(init_min_clients, global_batch_size, witness_nodes).await;

    assert_with_retries(|| server_handle.get_clients_len(), 0).await;
    assert_with_retries(
        || server_handle.get_run_state(),
        RunState::WaitingForMembers,
    )
    .await;

    let training_delay = 2;
    let server_port = server_handle.server_port;
    let run_id = &server_handle.run_id;

    let _client_handles = spawn_clients_with_training_delay(
        init_min_clients as usize,
        server_port,
        run_id,
        training_delay,
    )
    .await;

    info!("waiting for init min clients...");
    assert_with_retries(
        || server_handle.get_clients_len(),
        init_min_clients as usize,
    )
    .await;

    // execute round 0
    info!("waiting for round 0...");
    assert_with_retries(|| server_handle.get_rounds_head(), 0).await;

    // warmup
    info!("waiting for warmup...");
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    // train
    info!("waiting for start of train...");
    assert_with_retries(|| server_handle.get_run_state(), RunState::RoundTrain).await;

    info!("waiting for round 1...");
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;

    // spawn new client
    let _clients_handle =
        spawn_clients_with_training_delay(2, server_port, run_id, training_delay).await;

    info!("waiting for next epoch!");
    assert_with_retries(|| server_handle.get_current_epoch(), 1).await;

    assert_with_retries(
        || server_handle.get_checkpoint(),
        std::mem::discriminant(&Checkpoint::P2PDummy),
    )
    .await;

    // check that the run state evolves naturally to Warmup where the model gets shared
    assert_with_retries(|| server_handle.get_run_state(), RunState::Warmup).await;

    info!("waiting for end of round!");
    assert_with_retries(|| server_handle.get_rounds_head(), 1).await;

    info!("waiting for next epoch!");
    assert_with_retries(|| server_handle.get_current_epoch(), 2).await;

    // check that the clients length shows the new joined client trained with new p2p shared model
    assert_with_retries(|| server_handle.get_clients_len(), 4).await;
}
