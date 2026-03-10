use std::{sync::Arc, time::Duration};

use psyche_decentralized_testing::{
    CLIENT_PROCESS_PREFIX, VALIDATOR_PROCESS_NAME,
    chaos::{ChaosAction, ChaosScheduler},
    subprocess_setup::e2e_testing_setup,
    subprocess_watcher::{Response, SubprocessWatcher},
    utils::SolanaTestClient,
};

use rstest::*;
use serial_test::serial;
use tokio::time;

#[ignore = "These tests are a bit flaky, so we need to make sure they work properly."]
#[rstest]
#[trace]
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_pause_solana_validator(
    #[values(1, 2)] n_clients: u8,
    #[values(0, 10)] pause_step: u64,
) {
    let run_id = "test".to_string();
    let num_of_epochs_to_run = 2;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;

    let mut watcher = SubprocessWatcher::new();
    let watcher_arc = Arc::new(SubprocessWatcher::new());

    let _cleanup = e2e_testing_setup(&watcher, n_clients as usize).await;

    let solana_client = Arc::new(SolanaTestClient::new(run_id, None).await);

    tokio::time::sleep(Duration::from_secs(10)).await;

    let chaos_targets = vec![VALIDATOR_PROCESS_NAME.to_string()];

    let chaos_scheduler = ChaosScheduler::new(watcher_arc, solana_client);
    chaos_scheduler
        .schedule_chaos(
            ChaosAction::Pause {
                duration_secs: 60,
                targets: chaos_targets.clone(),
            },
            pause_step,
        )
        .await;

    let mut liveness_check_interval = time::interval(Duration::from_secs(10));
    println!("Train starting");

    loop {
        tokio::select! {
           _ = liveness_check_interval.tick() => {
               if let Err(e) = watcher.monitor_clients_health(n_clients).await {
                   panic!("{}", e);
              }
           }
           response = watcher.log_rx.recv() => {
               if let Some(Response::Loss(client, epoch, step, loss)) = response {
                   let loss = loss.unwrap();
                   println!(
                       "client: {client:?}, epoch: {epoch}, step: {step}, Loss: {loss}"
                   );
                   if epoch as i64 > current_epoch {
                       current_epoch = epoch as i64;
                       assert!(loss < last_epoch_loss);
                       last_epoch_loss = loss;
                       if epoch == num_of_epochs_to_run {
                           break;
                       }
                   }
               }
           }
        }
    }
}

#[ignore = "Delay chaos action not supported in subprocess mode (needs CAP_NET_ADMIN)"]
#[rstest]
#[trace]
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_delay_solana_test_validator(
    #[values(1, 2)] n_clients: u8,
    #[values(0, 10)] delay_step: u64,
    #[values(1000, 5000)] delay_milis: i64,
) {
    let run_id = "test".to_string();
    let num_of_epochs_to_run = 2;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;

    let mut watcher = SubprocessWatcher::new();
    let watcher_arc = Arc::new(SubprocessWatcher::new());

    let _cleanup = e2e_testing_setup(&watcher, n_clients as usize).await;

    let solana_client = Arc::new(SolanaTestClient::new(run_id, None).await);

    tokio::time::sleep(Duration::from_secs(10)).await;

    let chaos_targets = vec![VALIDATOR_PROCESS_NAME.to_string()];

    let chaos_scheduler = ChaosScheduler::new(watcher_arc, solana_client);
    chaos_scheduler
        .schedule_chaos(
            ChaosAction::Delay {
                duration_secs: 120,
                latency_ms: delay_milis,
                targets: chaos_targets.clone(),
            },
            delay_step,
        )
        .await;

    let mut liveness_check_interval = time::interval(Duration::from_secs(10));
    println!("Train starting");

    loop {
        tokio::select! {
           _ = liveness_check_interval.tick() => {
                   if let Err(e) = watcher.monitor_clients_health(n_clients).await {
                       panic!("{}", e);
               }
           }
           response = watcher.log_rx.recv() => {
               if let Some(Response::Loss(client, epoch, step, loss)) = response {
                   let loss = loss.unwrap();
                   println!(
                       "client: {client:?}, epoch: {epoch}, step: {step}, Loss: {loss}"
                   );
                   if epoch as i64 > current_epoch {
                       current_epoch = epoch as i64;
                       assert!(loss < last_epoch_loss);
                       last_epoch_loss = loss;
                       if epoch == num_of_epochs_to_run {
                           break;
                       }
                   }
               }
           }
        }
    }
}

#[ignore = "Delay chaos action not supported in subprocess mode (needs CAP_NET_ADMIN)"]
#[rstest]
#[trace]
#[test_log::test(tokio::test(flavor = "multi_thread"))]
#[serial]
async fn test_delay_solana_client(#[values(1, 2)] n_clients: u8, #[values(0, 10)] delay_step: u64) {
    let run_id = "test".to_string();
    let num_of_epochs_to_run = 2;
    let mut current_epoch = -1;
    let mut last_epoch_loss = f64::MAX;

    let mut watcher = SubprocessWatcher::new();
    let watcher_arc = Arc::new(SubprocessWatcher::new());

    let _cleanup = e2e_testing_setup(&watcher, n_clients as usize).await;

    let solana_client = Arc::new(SolanaTestClient::new(run_id, None).await);

    tokio::time::sleep(Duration::from_secs(10)).await;

    let chaos_targets = (1..=n_clients)
        .map(|i| format!("{CLIENT_PROCESS_PREFIX}-{i}"))
        .collect::<Vec<String>>();

    let chaos_scheduler = ChaosScheduler::new(watcher_arc, solana_client);
    chaos_scheduler
        .schedule_chaos(
            ChaosAction::Delay {
                duration_secs: 120,
                latency_ms: 1000,
                targets: chaos_targets.clone(),
            },
            delay_step,
        )
        .await;

    let mut liveness_check_interval = time::interval(Duration::from_secs(10));
    println!("Train starting");
    loop {
        tokio::select! {
           _ = liveness_check_interval.tick() => {
               if let Err(e) = watcher.monitor_clients_health(n_clients).await {
                   panic!("{}", e);
              }
           }
           response = watcher.log_rx.recv() => {
               if let Some(Response::Loss(client, epoch, step, loss)) = response {
                   let loss = loss.unwrap();
                   println!(
                       "client: {client:?}, epoch: {epoch}, step: {step}, Loss: {loss}"
                   );

                   if epoch as i64 > current_epoch {
                       current_epoch = epoch as i64;
                       assert!(loss < last_epoch_loss);
                       last_epoch_loss = loss;
                       if epoch == num_of_epochs_to_run {
                           break;
                       }
                   }
               }
           }
        }
    }
}
