use std::sync::Arc;
use std::time::Duration;

use anyhow::{Error, Result};
use iroh::Endpoint;
use iroh_n0des::Registry;
use iroh_n0des::simulation::Node;
use iroh_n0des::simulation::SetupData;
use iroh_n0des::simulation::Spawn;
use iroh_n0des::simulation::SpawnContext;
use psyche_centralized_client::app::App as ClientApp;
use psyche_centralized_client::app::AppBuilder as ClientAppBuilder;
use psyche_centralized_shared::ClientId;
use psyche_client::NC;
use psyche_client::RunInitConfig;
use psyche_network::NetworkConnection;
use psyche_network::allowlist;
use psyche_network::router::Router;
use serde::{Deserialize, Serialize};
use tokio::select;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::test_utils::dummy_client_app_params_default;
use crate::test_utils::dummy_client_app_params_with_training_delay;

pub struct Client {
    inner: ClientApp,
}

impl Client {
    pub async fn default(
        server_port: u16,
        run_id: &str,
    ) -> (
        Self,
        allowlist::AllowDynamic,
        NC,
        RunInitConfig<ClientId, ClientId>,
    ) {
        let client_app_params = dummy_client_app_params_default(server_port, run_id);
        let (client_app, allowlist, p2p, state_options) = ClientAppBuilder::new(client_app_params)
            .build()
            .await
            .unwrap();

        (Self { inner: client_app }, allowlist, p2p, state_options)
    }

    pub async fn new_with_training_delay(
        server_port: u16,
        run_id: &str,
        training_delay_secs: u64,
        endpoint: Option<Endpoint>,
    ) -> (
        Self,
        allowlist::AllowDynamic,
        NC,
        RunInitConfig<ClientId, ClientId>,
    ) {
        let client_app_params = dummy_client_app_params_with_training_delay(
            server_port,
            run_id,
            training_delay_secs,
            endpoint,
        );
        let (client_app, allowlist, p2p, state_options) = ClientAppBuilder::new(client_app_params)
            .build()
            .await
            .unwrap();
        (Self { inner: client_app }, allowlist, p2p, state_options)
    }

    pub async fn run(
        &mut self,
        allowlist: allowlist::AllowDynamic,
        p2p: NC,
        state_options: RunInitConfig<ClientId, ClientId>,
    ) -> Result<()> {
        debug!(
            "spawned new client: {}",
            p2p.node_addr().await.unwrap().node_id
        );
        let client_run = self.inner.run(allowlist, p2p, state_options);
        tokio::pin!(client_run);
        loop {
            select! {
                run_res = &mut client_run => run_res?,
            }
        }
    }
}

impl Node for Client {
    fn endpoint(&self) -> Option<&Endpoint> {
        Some(self.inner.router.endpoint())
    }

    async fn shutdown(&mut self) -> Result<()> {
        self.inner.router.shutdown().await?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Setup {
    pub server_port: u16,
    pub training_delay_secs: u64,
    pub run_id: String,
}

impl Spawn<Setup> for Client {
    async fn spawn(ctx: &mut SpawnContext<'_, Setup>) -> Result<Self> {
        let endpoint = ctx.bind_endpoint().await?;
        let setup_data = ctx.setup_data().clone();
        let registry = ctx.metrics_registry();
        let (mut node, allow_list, p2p, state_options) = Client::new_with_training_delay(
            setup_data.server_port,
            &setup_data.run_id,
            setup_data.training_delay_secs,
            Some(endpoint),
        )
        .await;
        // registry.register_all_prefixed(endpoint.metrics());
        // registry.register(p2p.blobs.metrics().clone());
        // registry.register(p2p.gossip.metrics().clone());
        node.run(allow_list, p2p, state_options).await?;

        Ok(node)
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ClientHandle {
    pub client_handle: JoinHandle<Result<(), Error>>,
    pub router: Arc<Router>,
}

impl ClientHandle {
    pub async fn default(server_port: u16, run_id: &str) -> Self {
        let (mut client, allowlist, p2p, state_options) =
            Client::default(server_port, run_id).await;
        let router = client.inner.router.clone();
        let client_handle =
            tokio::spawn(async move { client.run(allowlist, p2p, state_options).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        Self {
            client_handle,
            router,
        }
    }

    pub async fn new_with_training_delay(
        server_port: u16,
        run_id: &str,
        training_delay_secs: u64,
        sim_endpoint: Option<Endpoint>,
    ) -> Self {
        debug!("spawning new client...");
        let (mut client, allowlist, p2p, state_options) =
            Client::new_with_training_delay(server_port, run_id, training_delay_secs, sim_endpoint)
                .await;
        let router = client.inner.router.clone();
        let client_handle =
            tokio::spawn(async move { client.run(allowlist, p2p, state_options).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        debug!("new client spawned!");
        Self {
            client_handle,
            router,
        }
    }
}

impl Node for ClientHandle {
    fn endpoint(&self) -> Option<&Endpoint> {
        Some(self.router.endpoint())
    }

    async fn shutdown(&mut self) -> Result<()> {
        self.router.shutdown().await?;
        Ok(())
    }
}

impl Spawn<Setup> for ClientHandle {
    async fn spawn(ctx: &mut SpawnContext<'_, Setup>) -> Result<Self> {
        // let endpoint = ctx.bind_endpoint().await?;
        let setup_data = ctx.setup_data().clone();
        let registry = ctx.metrics_registry();
        let mut handle = ClientHandle::new_with_training_delay(
            setup_data.server_port,
            &setup_data.run_id,
            setup_data.training_delay_secs,
            None,
        )
        .await;
        // registry.register_all_prefixed(endpoint.metrics());
        // registry.register(p2p.blobs.metrics().clone());
        // registry.register(p2p.gossip.metrics().clone());

        Ok(handle)
    }
}
