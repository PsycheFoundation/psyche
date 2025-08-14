use anyhow::ensure;
use iroh::NodeAddr;
use iroh_blobs::net_protocol::Blobs;
use iroh_n0des::simulation::{Builder, DynNode, RoundContext, Spawn};
use psyche_centralized_testing::{
    COOLDOWN_TIME, MAX_ROUND_TRAIN_TIME, ROUND_WITNESS_TIME,
    client::{Client, ClientHandle, ClientHandleWithDelay},
    server::CoordinatorServerHandle,
    test_utils::{
        Setup, assert_with_retries, assert_witnesses_healthy_score, spawn_clients,
        spawn_clients_with_training_delay,
    },
};
use psyche_coordinator::{
    RunState,
    model::{Checkpoint, HubRepo},
};
use rand::seq::IteratorRandom;
use std::time::Duration;
use tracing::info;

#[iroh_n0des::sim]
async fn finish_epoch() -> anyhow::Result<Builder<Setup>> {
    async fn round(node: &mut ClientHandle, ctx: &RoundContext<'_, Setup>) -> anyhow::Result<bool> {
        println!("Node moving on");
        Ok(true)
    }

    fn check(node: &ClientHandle, ctx: &RoundContext<'_, Setup>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn coordinator_round(
        node: &mut CoordinatorServerHandle,
        ctx: &RoundContext<'_, Setup>,
    ) -> anyhow::Result<bool> {
        println!("RUN STATE BEFORE TRAINING: {}", node.get_run_state().await);
        loop {
            if node.get_run_state().await == RunState::RoundTrain {
                break;
            }
        }
        println!("RUN STATE AFTER TRAINING: {}", node.get_run_state().await);
        loop {
            if node.get_run_state().await == RunState::RoundWitness {
                tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;
                break;
            }
        }
        println!("RUN STATE AFTER WITNESS: {}", node.get_run_state().await);
        Ok(true)
    }

    // We initialize the coordinator with the same number of min clients as batches per round.
    // This way, every client will be assigned with only one batch
    let init_min_clients = 10;
    let global_batch_size = 10;
    let witness_nodes = 10;

    let port = 51000;
    let run_id = String::from("test_run");
    let sim = Builder::with_setup(async move || {
        let setup = Setup {
            training_delay_secs: 1000,
            server_port: port,
            run_id: run_id.clone(),
            init_min_clients: init_min_clients,
            global_batch_size: global_batch_size,
            witness_nodes: witness_nodes,
        };
        Ok(setup)
    })
    .spawn(1, CoordinatorServerHandle::builder(coordinator_round))
    .spawn(10, ClientHandle::builder(round))
    .rounds(4);
    Ok(sim)
}

#[iroh_n0des::sim]
async fn p2p_simulation() -> anyhow::Result<Builder<Setup>> {
    async fn client_round(
        node: &mut ClientHandle,
        ctx: &RoundContext<'_, Setup>,
    ) -> anyhow::Result<bool> {
        println!("Node moving on");
        Ok(true)
    }

    async fn delay_client_round(
        node: &mut ClientHandleWithDelay,
        ctx: &RoundContext<'_, Setup>,
    ) -> anyhow::Result<bool> {
        if ctx.round() == 0 {
            println!("Round of delayed node");
        }
        Ok(true)
    }

    fn check(node: &ClientHandle, ctx: &RoundContext<'_, Setup>) -> anyhow::Result<()> {
        Ok(())
    }

    async fn coordinator_round(
        node: &mut CoordinatorServerHandle,
        ctx: &RoundContext<'_, Setup>,
    ) -> anyhow::Result<bool> {
        println!("RUN STATE BEFORE TRAINING: {}", node.get_run_state().await);
        loop {
            if node.get_run_state().await == RunState::RoundTrain {
                break;
            }
        }
        println!("RUN STATE AFTER TRAINING: {}", node.get_run_state().await);
        loop {
            if node.get_run_state().await == RunState::RoundWitness {
                tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;
                break;
            }
        }
        println!("RUN STATE AFTER WITNESS: {}", node.get_run_state().await);
        if ctx.round() == 9 {
            loop {
                if node.get_run_state().await == RunState::Finished {
                    println!("Two epochs completed");
                    break;
                }
            }
        }
        Ok(true)
    }

    // We initialize the coordinator with the same number of min clients as batches per round.
    // This way, every client will be assigned with only one batch
    let init_min_clients = 5;
    let global_batch_size = 20;
    let witness_nodes = 5;

    let port = 51000;
    let run_id = String::from("test_run");
    let sim = Builder::with_setup(async move || {
        let setup = Setup {
            training_delay_secs: 1000,
            server_port: port,
            run_id: run_id.clone(),
            init_min_clients: init_min_clients,
            global_batch_size: global_batch_size,
            witness_nodes: witness_nodes,
        };
        Ok(setup)
    })
    .spawn(1, CoordinatorServerHandle::builder(coordinator_round))
    .spawn(5, ClientHandle::builder(client_round))
    .spawn(15, ClientHandleWithDelay::builder(delay_client_round))
    .rounds(10);
    Ok(sim)
}
