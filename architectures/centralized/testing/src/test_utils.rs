use std::future::Future;
use std::time::Duration;

use psyche_centralized_client::app::AppParams;
use psyche_coordinator::{assign_data_for_state, get_batch_ids_for_node, CommitteeSelection};
use psyche_network::{DiscoveryMode, SecretKey};
use rand::distributions::{Alphanumeric, DistString};
use std::env;
use tokio_util::sync::CancellationToken;

use crate::client::ClientHandle;
use crate::server::CoordinatorServerHandle;

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

pub async fn assert_with_retries<T, F, Fut>(mut function: F, y: T)
where
    T: PartialEq + std::fmt::Debug,
    Fut: Future<Output = T>,
    F: FnMut() -> Fut,
{
    let retry_attempts: u64 = 10;
    let mut result;
    for attempt in 1..=retry_attempts {
        result = function().await;
        if result == y {
            return;
        } else if attempt == retry_attempts {
            panic!("assertion failed {:?} != {:?}", result, y);
        } else {
            tokio::time::sleep(Duration::from_millis(250 * attempt)).await;
        }
    }
}

pub fn sample_rand_run_id() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 16)
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

    let coordinator = server_handle.get_coordinator().await;
    let committee_selection = CommitteeSelection::from_coordinator(&coordinator, true).unwrap();
    let data_assignments = assign_data_for_state(&coordinator, true, &committee_selection);

    // calculate score
    let mut score = 0;
    clients.iter().for_each(|client| {
        let batch_ids = get_batch_ids_for_node(
            &data_assignments,
            &client.id,
            coordinator.config.data_indicies_per_batch,
        );

        score += psyche_coordinator::Coordinator::trainer_healthy_score_by_witnesses(
            &batch_ids, &client.id, witnesses,
        );
    });

    assert_eq!(
        score, expected_score,
        "Score {} != expected score {}",
        score, expected_score
    );
}

pub fn dummy_client_app_params_with_training_delay(
    server_port: u16,
    run_id: &str,
    training_delay_secs: u64,
) -> AppParams {
    AppParams {
        cancel: CancellationToken::default(),
        identity_secret_key: SecretKey::generate(&mut rand::rngs::OsRng),
        server_addr: format!("localhost:{}", server_port).to_string(),
        tx_tui_state: None,
        run_id: run_id.to_string(),
        data_parallelism: 1,
        tensor_parallelism: 1,
        micro_batch_size: None,
        write_gradients_dir: None,
        p2p_port: None,
        eval_tasks: Vec::new(),
        eval_task_max_docs: None,
        checkpoint_upload_info: None,
        hub_read_token: None,
        wandb_info: None,
        optim_stats: None,
        grad_accum_in_fp32: false,
        dummy_training_delay_secs: Some(training_delay_secs),
        discovery_mode: DiscoveryMode::Local,
        max_concurrent_parameter_requests: 10,
    }
}

pub fn dummy_client_app_params_default(server_port: u16, run_id: &str) -> AppParams {
    AppParams {
        cancel: CancellationToken::default(),
        identity_secret_key: SecretKey::generate(&mut rand::rngs::OsRng),
        server_addr: format!("localhost:{}", server_port).to_string(),
        tx_tui_state: None,
        run_id: run_id.to_string(),
        data_parallelism: 1,
        tensor_parallelism: 1,
        micro_batch_size: None,
        write_gradients_dir: None,
        p2p_port: None,
        eval_tasks: Vec::new(),
        eval_task_max_docs: None,
        checkpoint_upload_info: None,
        hub_read_token: None,
        wandb_info: None,
        optim_stats: None,
        grad_accum_in_fp32: false,
        dummy_training_delay_secs: None,
        discovery_mode: DiscoveryMode::Local,
        max_concurrent_parameter_requests: 10,
    }
}
