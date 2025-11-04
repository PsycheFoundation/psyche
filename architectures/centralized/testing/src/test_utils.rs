use std::future::Future;
use std::time::Duration;

use crate::client::ClientHandle;
use crate::server::CoordinatorServerHandle;
use iroh::Endpoint;
use iroh_n0des::Registry;
use psyche_centralized_client::app::AppParams;
use psyche_network::{DiscoveryMode, SecretKey};
use rand::distr::{Alphanumeric, SampleString};
use serde::{Deserialize, Serialize};
use std::env;
use tokio::task::JoinHandle;
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
) -> Vec<JoinHandle<()>> {
    let mut client_handles = Vec::new();
    for _ in 0..num_clients {
        let mut client_handle = ClientHandle::default(server_port, run_id).await;
        let handle = client_handle.run_client().await.unwrap();
        client_handles.push(handle);
    }
    client_handles
}

pub async fn spawn_clients_with_training_delay(
    num_clients: usize,
    server_port: u16,
    run_id: &str,
    training_delay_secs: u64,
) -> Vec<JoinHandle<()>> {
    let mut client_handles = Vec::new();
    for _ in 0..num_clients {
        let mut client_handle = ClientHandle::new_with_training_delay(
            server_port,
            run_id,
            training_delay_secs,
            None,
            None,
        )
        .await;
        let handle = client_handle.run_client().await.unwrap();
        client_handles.push(handle)
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

pub fn dummy_client_app_params_with_training_delay(
    server_port: u16,
    run_id: &str,
    training_delay_secs: u64,
    sim_endpoint: Option<Endpoint>,
) -> AppParams {
    AppParams {
        cancel: CancellationToken::default(),
        identity_secret_key: SecretKey::generate(&mut rand::rng()),
        server_addr: format!("localhost:{server_port}").to_string(),
        tx_tui_state: None,
        run_id: run_id.to_string(),
        data_parallelism: 1,
        tensor_parallelism: 1,
        micro_batch_size: 1,
        write_gradients_dir: None,
        p2p_port: None,
        p2p_interface: None,
        eval_tasks: Vec::new(),
        prompt_task: false,
        eval_task_max_docs: None,
        checkpoint_upload_info: None,
        hub_read_token: None,
        hub_max_concurrent_downloads: 1,
        wandb_info: None,
        optim_stats: None,
        grad_accum_in_fp32: false,
        dummy_training_delay_secs: Some(training_delay_secs),
        discovery_mode: DiscoveryMode::Local,
        max_concurrent_parameter_requests: 10,
        metrics_local_port: None,
        sim_endpoint,
        device: Default::default(),
        sidecar_port: Default::default(),
    }
}

pub fn dummy_client_app_params_default(server_port: u16, run_id: &str) -> AppParams {
    AppParams {
        cancel: CancellationToken::default(),
        identity_secret_key: SecretKey::generate(&mut rand::rng()),
        server_addr: format!("localhost:{server_port}").to_string(),
        tx_tui_state: None,
        run_id: run_id.to_string(),
        data_parallelism: 1,
        tensor_parallelism: 1,
        micro_batch_size: 1,
        write_gradients_dir: None,
        p2p_port: None,
        p2p_interface: None,
        eval_tasks: Vec::new(),
        eval_task_max_docs: None,
        prompt_task: false,
        checkpoint_upload_info: None,
        hub_read_token: None,
        hub_max_concurrent_downloads: 1,
        wandb_info: None,
        optim_stats: None,
        grad_accum_in_fp32: false,
        dummy_training_delay_secs: None,
        discovery_mode: DiscoveryMode::Local,
        max_concurrent_parameter_requests: 10,
        metrics_local_port: None,
        sim_endpoint: None,
        device: Default::default(),
        sidecar_port: Default::default(),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Setup {
    pub server_port: u16,
    pub training_delay_secs: u64,
    pub init_min_clients: u16,
    pub global_batch_size: u16,
    pub witness_nodes: u16,
    pub run_id: String,
}
