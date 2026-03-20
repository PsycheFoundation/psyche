use std::future::Future;
use std::time::Duration;

use crate::client::ClientHandle;
use crate::server::CoordinatorServerHandle;
use clap::Parser;
use psyche_client::TrainArgs;
use rand::distr::{Alphanumeric, SampleString};
use std::env;
use tokio_util::sync::CancellationToken;

pub fn repo_path() -> String {
    let cargo_manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    std::path::Path::new(&cargo_manifest_dir)
        .ancestors()
        .nth(3)
        .expect("Failed to determine repository root")
        .to_str()
        .unwrap()
        .to_string()
}

pub async fn spawn_clients(
    num_clients: usize,
    server_port: u16,
    run_id: &str,
) -> Vec<ClientHandle> {
    let mut client_handles = Vec::new();
    for _ in 0..num_clients {
        client_handles.push(ClientHandle::default(server_port, run_id).await)
    }
    client_handles
}

pub async fn spawn_clients_with_training_delay(
    num_clients: usize,
    server_port: u16,
    run_id: &str,
    training_delay_secs: u64,
) -> Vec<ClientHandle> {
    let mut client_handles = Vec::new();
    for _ in 0..num_clients {
        client_handles.push(
            ClientHandle::new_with_training_delay(server_port, run_id, training_delay_secs).await,
        )
    }
    client_handles
}

pub async fn assert_with_retries<T, F, Fut>(function: F, y: T)
where
    T: PartialEq + std::fmt::Debug,
    Fut: Future<Output = T>,
    F: FnMut() -> Fut,
{
    let res = with_retries(function, y).await;
    assert!(res);
}

pub async fn with_retries<T, F, Fut>(mut function: F, y: T) -> bool
where
    T: PartialEq + std::fmt::Debug,
    Fut: Future<Output = T>,
    F: FnMut() -> Fut,
{
    let retry_attempts: u64 = 100;
    let mut result;
    for attempt in 1..=retry_attempts {
        result = function().await;
        if result == y {
            return true;
        } else if attempt == retry_attempts {
            eprintln!("assertion failed, got: {result:?} but expected: {y:?}");
            return false;
        } else {
            tokio::time::sleep(Duration::from_millis(10 * attempt)).await;
        }
    }
    false
}

pub fn sample_rand_run_id() -> String {
    Alphanumeric.sample_string(&mut rand::rng(), 16)
}

/// Sums the healthy score of all nodes and assert it vs expected_score
pub async fn assert_witnesses_healthy_score(
    server_handle: &CoordinatorServerHandle,
    round_number: usize,
    expected_score: u16,
) {
    let clients = server_handle.get_clients().await;

    // get witnesses
    let rounds = server_handle.get_rounds().await;
    let witnesses = &rounds[round_number].witnesses;

    // calculate score
    let mut score = 0;
    clients.iter().for_each(|client| {
        score += psyche_coordinator::Coordinator::trainer_healthy_score_by_witnesses(
            &client.id, witnesses,
        );
    });

    assert_eq!(
        score, expected_score,
        "Score {score} != expected score {expected_score}"
    );
}

pub struct AppParams {
    pub(crate) cancel: CancellationToken,
    pub(crate) server_addr: String,
    pub(crate) train_args: TrainArgs,
}

#[derive(Parser)]
struct TestCli {
    #[command(flatten)]
    train_args: TrainArgs,
}

#[rustfmt::skip]
pub fn dummy_client_app_params_with_training_delay(
    server_port: u16,
    run_id: &str,
    training_delay_secs: u64,
) -> AppParams {
    std::env::set_var("HF_TOKEN", "dummy_token");
    AppParams {
        cancel: CancellationToken::default(),
        server_addr: format!("localhost:{server_port}").to_string(),
        train_args: TestCli::parse_from([
            "dummy",
            "--run-id", run_id,
            "--iroh-relay", "disabled",
            "--iroh-discovery", "local",
            "--data-parallelism", "1",
            "--tensor-parallelism", "1",
            "--micro-batch-size", "1",
            "--max-concurrent-parameter-requests", "10",
            "--hub-max-concurrent-downloads", "1",
            "--dummy-training-delay-secs", training_delay_secs.to_string().as_str(),
            "--skip-checkpoint-upload",
        ])
        .train_args,
    }
}
