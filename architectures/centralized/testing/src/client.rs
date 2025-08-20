use std::time::Duration;

use anyhow::{Error, Result};
use psyche_centralized_client::app::App as ClientApp;
use psyche_centralized_shared::ClientId;
use psyche_client::NC;
use psyche_client::RunInitConfig;
use psyche_network::allowlist;
use tokio::select;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::test_utils::dummy_client_app_params_default;
use crate::test_utils::dummy_client_app_params_with_training_delay;

struct Client {
    inner: ClientApp,
}

impl Client {
    pub async fn default(server_port: u16, run_id: &str) -> Self {
        let client_app_params = dummy_client_app_params_default(server_port, run_id);
        let client_app = ClientApp::new(client_app_params).await.unwrap();

        Self { inner: client_app }
    }

    pub async fn new_with_training_delay(
        server_port: u16,
        run_id: &str,
        training_delay_secs: u64,
    ) -> Self {
        let client_app_params =
            dummy_client_app_params_with_training_delay(server_port, run_id, training_delay_secs);
        let client_app = ClientApp::new(client_app_params).await.unwrap();

        Self { inner: client_app }
    }

    pub async fn run(self) -> Result<()> {
        let client_run = self.inner.run();
        tokio::pin!(client_run);
        loop {
            select! {
                run_res = &mut client_run => run_res?,
            }
        }
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ClientHandle {
    pub client_handle: JoinHandle<Result<(), Error>>,
}

impl ClientHandle {
    pub async fn default(server_port: u16, run_id: &str) -> Self {
        let client = Client::default(server_port, run_id).await;
        let client_handle = tokio::spawn(async move { client.run().await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        Self { client_handle }
    }

    pub async fn new_with_training_delay(
        server_port: u16,
        run_id: &str,
        training_delay_secs: u64,
    ) -> Self {
        debug!("spawning new client...");
        let client =
            Client::new_with_training_delay(server_port, run_id, training_delay_secs).await;
        let client_handle = tokio::spawn(async move { client.run().await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        debug!("new client spawned!");
        Self { client_handle }
    }
}
