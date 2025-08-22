use anyhow::ensure;
use iroh::NodeAddr;
use iroh_blobs::net_protocol::Blobs;
use iroh_n0des::simulation::{Builder, DynNode, RoundContext, Spawn};
use psyche_centralized_testing::{
    client::{Client, ClientHandle},
    server::CoordinatorServerHandle,
    test_utils::{
        assert_with_retries, assert_witnesses_healthy_score, spawn_clients,
        spawn_clients_with_training_delay, Setup,
    },
    COOLDOWN_TIME, MAX_ROUND_TRAIN_TIME, ROUND_WITNESS_TIME,
};
use psyche_coordinator::{
    model::{Checkpoint, HubRepo},
    RunState,
};
use rand::seq::IteratorRandom;
use std::time::Duration;
use tracing::info;

#[iroh_n0des::sim]
async fn finish_epoch() -> anyhow::Result<Builder<Setup>> {
    async fn round(node: &mut ClientHandle, ctx: &RoundContext<'_, Setup>) -> anyhow::Result<bool> {
        if ctx.round() == 0 {
            println!("Spawning new node");
            node.run_client().await.unwrap();
        } else {
            println!("Node moving on in round: {}", ctx.round());
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
        if ctx.round() == 0 {
            loop {
                if node.get_run_state().await == RunState::WaitingForMembers {
                    println!("Coordinator is waiting for members to join");
                    break;
                }
            }
        } else {
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
        }
        Ok(true)
    }

    // We initialize the coordinator with the same number of min clients as batches per round.
    // This way, every client will be assigned with only one batch
    let init_min_clients = 10;
    let global_batch_size = 10;
    let witness_nodes = 0;

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
    .rounds(5);
    Ok(sim)
}

#[iroh_n0des::sim]
async fn p2p_simulation() -> anyhow::Result<Builder<Setup>> {
    async fn client_round(
        node: &mut ClientHandle,
        ctx: &RoundContext<'_, Setup>,
    ) -> anyhow::Result<bool> {
        if ctx.round() == 0 {
            println!("Spawning new node");
            node.run_client().await.unwrap();
        } else {
            println!("Node moving on in round: {}", ctx.round());
        }
        Ok(true)
    }

    async fn late_join_client_round(
        node: &mut ClientHandle,
        ctx: &RoundContext<'_, Setup>,
    ) -> anyhow::Result<bool> {
        if ctx.round() == 2 {
            println!("Spawning new nodes for next psyche round");
            node.run_client().await.unwrap();
        } else {
            println!("New node moving on in round: {}", ctx.round());
        }
        Ok(true)
    }

    async fn coordinator_round(
        node: &mut CoordinatorServerHandle,
        ctx: &RoundContext<'_, Setup>,
    ) -> anyhow::Result<bool> {
        if ctx.round() == 0 {
            loop {
                if node.get_run_state().await == RunState::WaitingForMembers {
                    println!("Coordinator is waiting for members to join");
                    break;
                }
            }
        } else {
            loop {
                if node.get_run_state().await == RunState::RoundTrain {
                    println!("Coordinator is in training state");
                    break;
                } else if node.get_run_state().await == RunState::Cooldown {
                    println!("Coordinator went to cooldown state");
                }
            }
            loop {
                if node.get_run_state().await == RunState::RoundWitness {
                    println!("Coordinator is in witness state");
                    tokio::time::sleep(Duration::from_secs(ROUND_WITNESS_TIME)).await;
                    break;
                }
            }
        }
        if ctx.round() == 9 {
            loop {
                if node.get_run_state().await == RunState::Finished {
                    println!("We went through two epochs");
                    break;
                }
            }
        }
        Ok(true)
    }

    // We initialize the coordinator with the same number of min clients as batches per round.
    // This way, every client will be assigned with only one batch
    let init_min_clients = 36;
    let global_batch_size = 60;
    let witness_nodes = 0;

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
    .spawn(36, ClientHandle::builder(client_round))
    .spawn(24, ClientHandle::builder(late_join_client_round))
    .rounds(10);
    Ok(sim)
}
