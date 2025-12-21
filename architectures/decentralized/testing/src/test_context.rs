use crate::{
    CLIENT_CONTAINER_PREFIX,
    docker_setup::{DockerTestCleanup, e2e_testing_setup, monitor_client},
    docker_watcher::DockerWatcher,
    utils::SolanaTestClient,
};
use bollard::Docker;
use std::{sync::Arc, time::Duration};
use tokio::time::{Interval, interval};

// Some common values we use across tests
// If some are flaky etc. you may try adjusting some of these
pub const DEFAULT_RUN_ID: &str = "test";
pub const DEFAULT_EPOCHS: u64 = 3;
pub const LIVENESS_CHECK_INTERVAL_SECS: u64 = 10;
pub const LOSS_IMPROVEMENT_THRESHOLD: f64 = 1.1;
pub const INITIAL_RUN_WAIT_SECS: u64 = 30;
pub const CLIENT_JOIN_WAIT_SECS: u64 = 20;
pub const STATE_TRANSITION_WAIT_SECS: u64 = 5;

pub struct TestContext {
    pub docker: Arc<Docker>,
    pub watcher: DockerWatcher,
    pub solana_client: SolanaTestClient,
    _cleanup: DockerTestCleanup,
}

impl TestContext {
    pub async fn new(n_clients: usize) -> Self {
        let docker = Arc::new(Docker::connect_with_socket_defaults().unwrap());
        let watcher = DockerWatcher::new(docker.clone());
        let cleanup = e2e_testing_setup(docker.clone(), n_clients).await;
        let solana_client = SolanaTestClient::new(DEFAULT_RUN_ID.to_string()).await;

        Self {
            docker,
            watcher,
            solana_client,
            _cleanup: cleanup,
        }
    }

    /// Monitor all clients from 1 to count
    pub fn monitor_all_clients(&self, count: usize) {
        self.monitor_clients(1, count)
    }

    /// Monitor cliens from start..count
    fn monitor_clients(&self, start: usize, count: usize) {
        for i in start..start + count {
            monitor_client(&self.watcher, &format!("{CLIENT_CONTAINER_PREFIX}-{i}"))
                .expect("Failed to monitor client");
        }
    }
}

pub fn create_liveness_ticker() -> Interval {
    interval(Duration::from_secs(LIVENESS_CHECK_INTERVAL_SECS))
}
