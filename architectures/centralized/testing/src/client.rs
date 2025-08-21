use std::sync::Arc;
use std::time::Duration;

use anyhow::{Error, Result};
use psyche_centralized_client::app::App as ClientApp;
use psyche_centralized_client::app::AppParams;
use psyche_centralized_shared::ClientId;
use psyche_client::NC;
use psyche_client::RunInitConfig;
use psyche_network::allowlist;
use psyche_network::router::Router;
use tokio::select;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::test_utils::dummy_client_app_params_default;
use crate::test_utils::dummy_client_app_params_with_training_delay;

#[derive(Debug)]
struct Client {
    inner: ClientApp,
    params: AppParams,
}

impl Client {
    pub async fn default(server_port: u16, run_id: &str) -> Self {
        let client_app_params = dummy_client_app_params_default(server_port, run_id);
        let client_app = ClientApp::new(&client_app_params).await.unwrap();

        Self {
            inner: client_app,
            params: client_app_params,
        }
    }

    pub async fn new_with_training_delay(
        server_port: u16,
        run_id: &str,
        training_delay_secs: u64,
    ) -> Self {
        let client_app_params =
            dummy_client_app_params_with_training_delay(server_port, run_id, training_delay_secs);
        let client_app = ClientApp::new(&client_app_params).await.unwrap();

        Self {
            inner: client_app,
            params: client_app_params,
        }
    }

    pub async fn run(self) -> Result<()> {
        self.inner.run(self.params).await
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ClientHandle {
    pub client: Option<Client>,
    pub router: Arc<Router>,
}

impl ClientHandle {
    pub async fn default(server_port: u16, run_id: &str) -> Self {
        let client = Client::default(server_port, run_id).await;
        let router = client.inner.router.clone().unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        Self {
            client: Some(client),
            router,
        }
    }

    pub async fn new_with_training_delay(
        server_port: u16,
        run_id: &str,
        training_delay_secs: u64,
    ) -> Self {
        debug!("spawning new client...");
        let client =
            Client::new_with_training_delay(server_port, run_id, training_delay_secs).await;
        let router = client.inner.router.clone().unwrap();
        Self {
            client: Some(client),
            router,
        }
    }

    pub async fn run_client(&mut self) -> Result<JoinHandle<()>> {
        let client = self.client.take().unwrap();
        let handle = tokio::spawn(async move { client.run().await.unwrap() });
        Ok(handle)
    }
}
