use std::time::Duration;

use anyhow::{Error, Result};
use psyche_centralized_client::app::App as ClientApp;
use psyche_centralized_client::app::build_app as build_client_app;
use psyche_centralized_shared::ClientId;
use psyche_client::NC;
use psyche_client::RunInitConfig;
use psyche_network::allowlist;
use tokio::select;
use tokio::task::JoinHandle;
use tracing::debug;

use crate::test_utils::dummy_client_app_params_with_training_delay;

struct Client {
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
        let client_app_params = dummy_client_app_params_with_training_delay(server_port, run_id, 5);
        let (client_app, allowlist, p2p, state_options) = build_client_app(
            client_app_params.cancel,
            client_app_params.server_addr,
            None,
            client_app_params.train_args,
        )
        .await
        .unwrap();

        (Self { inner: client_app }, allowlist, p2p, state_options)
    }

    pub async fn new_with_training_delay(
        server_port: u16,
        run_id: &str,
        training_delay_secs: u64,
    ) -> (
        Self,
        allowlist::AllowDynamic,
        NC,
        RunInitConfig<ClientId, ClientId>,
    ) {
        let client_app_params =
            dummy_client_app_params_with_training_delay(server_port, run_id, training_delay_secs);
        let (client_app, allowlist, p2p, state_options) = build_client_app(
            client_app_params.cancel,
            client_app_params.server_addr,
            None,
            client_app_params.train_args,
        )
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
        debug!("spawned new client: {:?}", p2p.endpoint_addr().await);
        let client_run = self.inner.run(allowlist, p2p, state_options);
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
        let (mut client, allowlist, p2p, state_options) =
            Client::default(server_port, run_id).await;
        let client_handle =
            tokio::spawn(async move { client.run(allowlist, p2p, state_options).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        Self { client_handle }
    }

    pub async fn new_with_training_delay(
        server_port: u16,
        run_id: &str,
        training_delay_secs: u64,
    ) -> Self {
        debug!("spawning new client...");
        let (mut client, allowlist, p2p, state_options) =
            Client::new_with_training_delay(server_port, run_id, training_delay_secs).await;
        let client_handle =
            tokio::spawn(async move { client.run(allowlist, p2p, state_options).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        debug!("new client spawned!");
        Self { client_handle }
    }
}
