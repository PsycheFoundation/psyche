use std::sync::Arc;
use std::time::Duration;

use anyhow::{Error, Result};
use iroh::Endpoint;
use iroh::protocol::Router;
use iroh_n0des::Registry;
use iroh_n0des::simulation::Node;
use iroh_n0des::simulation::SetupData;
use iroh_n0des::simulation::Spawn;
use iroh_n0des::simulation::SpawnContext;
use psyche_centralized_client::app::App as ClientApp;
use psyche_centralized_client::app::AppParams;
use psyche_centralized_shared::ClientId;
use psyche_client::NC;
use psyche_client::RunInitConfig;
use psyche_network::NetworkConnection;
use psyche_network::allowlist;
use psyche_network::router;
use serde::{Deserialize, Serialize};
use tokio::select;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::server::CoordinatorServerHandle;
use crate::test_utils::Setup;
use crate::test_utils::dummy_client_app_params_default;
use crate::test_utils::dummy_client_app_params_with_training_delay;

#[derive(Debug)]
pub struct Client {
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
        endpoint: Option<Endpoint>,
    ) -> Self {
        let client_app_params = dummy_client_app_params_with_training_delay(
            server_port,
            run_id,
            training_delay_secs,
            endpoint,
        );
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
        sim_endpoint: Option<Endpoint>,
        registry: Option<&mut Registry>,
    ) -> Self {
        debug!("spawning new client...");
        let client =
            Client::new_with_training_delay(server_port, run_id, training_delay_secs, sim_endpoint)
                .await;

        let router = client.inner.router.clone().unwrap();
        // let gossip_metrics = client.inner.gossip_metrics.clone().unwrap();
        // let blob_metrics = client.inner.blob_metrics.clone().unwrap();

        // if let Some(registry) = registry {
        //     registry.register_all_prefixed(router.endpoint().metrics());
        //     registry.register(blob_metrics);
        //     registry.register(gossip_metrics);
        // }

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

impl Node for ClientHandle {
    fn endpoint(&self) -> Option<&iroh_n0des::iroh::Endpoint> {
        Some(self.router.endpoint())
    }

    async fn shutdown(&mut self) -> Result<()> {
        self.router.shutdown().await?;
        Ok(())
    }
}

impl Spawn<Setup> for ClientHandle {
    async fn spawn(ctx: &mut SpawnContext<'_, Setup>) -> Result<Self> {
        let setup_data = ctx.setup_data().clone();
        let registry = ctx.metrics_registry();
        let handle = ClientHandle::new_with_training_delay(
            setup_data.server_port,
            &setup_data.run_id,
            setup_data.training_delay_secs,
            None,
            Some(registry),
        )
        .await;

        Ok(handle)
    }
}
