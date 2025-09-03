use anyhow::{Error, Result};
use bytemuck::Zeroable;
use hf_hub::Repo;
use psyche_centralized_shared::{ClientId, ClientToServerMessage, ServerToClientMessage};
use psyche_client::{
    CheckpointConfig, Client, ClientTUI, ClientTUIState, NC, RunInitConfig, WandBInfo,
};
use psyche_coordinator::{Coordinator, HealthChecks, model};
use psyche_metrics::{BlobsMetrics, ClientMetrics, GossipMetrics, IrohMetricsRegistry};
use psyche_network::router::Router;
use psyche_network::{
    AuthenticatableIdentity, DiscoveryMode, Endpoint, NetworkTUIState, NetworkTui, NodeId,
    RelayMode, SecretKey, TcpClient, allowlist, psyche_relay_map,
};
use psyche_tui::logging::LoggerWidget;
use psyche_tui::{CustomWidget, TabbedWidget};
use psyche_watcher::{Backend as WatcherBackend, CoordinatorTui, OpportunisticData};
use std::sync::Arc;
use std::{path::PathBuf, time::Duration};
use tokio::sync::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender};
use tokio::time::interval;
use tokio::{select, sync::mpsc, time::Interval};
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub(super) type Tabs = TabbedWidget<(ClientTUI, CoordinatorTui, NetworkTui, LoggerWidget)>;
pub const TAB_NAMES: [&str; 4] = ["Client", "Coordinator", "Network", "Logger"];
type TabsData = <Tabs as CustomWidget>::Data;

pub enum ToSend {
    Witness(Box<OpportunisticData>),
    HealthCheck(HealthChecks<ClientId>),
    Checkpoint(model::HubRepo),
}

#[derive(Debug)]
struct Backend {
    allowlist: allowlist::AllowDynamic,
    rx: mpsc::UnboundedReceiver<Coordinator<ClientId>>,
    tx: mpsc::UnboundedSender<ToSend>,
}

#[async_trait::async_trait]
impl WatcherBackend<ClientId> for Backend {
    async fn wait_for_new_state(&mut self) -> Result<Coordinator<ClientId>> {
        let new_state = self
            .rx
            .recv()
            .await
            .ok_or(Error::msg("watcher backend rx channel closed"))?;
        self.allowlist.set(
            new_state
                .epoch_state
                .clients
                .iter()
                .map(|c| NodeId::from_bytes(c.id.get_p2p_public_key()).unwrap()),
        );
        Ok(new_state)
    }

    async fn send_witness(&mut self, opportunistic_data: OpportunisticData) -> Result<()> {
        Ok(self
            .tx
            .send(ToSend::Witness(Box::new(opportunistic_data)))?)
    }

    async fn send_health_check(&mut self, health_checks: HealthChecks<ClientId>) -> Result<()> {
        self.tx.send(ToSend::HealthCheck(health_checks))?;
        Ok(())
    }

    async fn send_checkpoint(&mut self, checkpoint: model::HubRepo) -> Result<()> {
        self.tx.send(ToSend::Checkpoint(checkpoint))?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct App {
    // Runtime state
    coordinator_state: Coordinator<ClientId>,
    update_tui_interval: Interval,
    cancel: CancellationToken,
    tx_tui_state: Option<Sender<TabsData>>,
    run_id: String,

    // Dependencies (initialized during run)
    server_conn: Option<TcpClient<ClientId, ClientToServerMessage, ServerToClientMessage>>,
    client: Option<Client<ClientId, ClientId, Backend>>,
    metrics: Option<Arc<ClientMetrics>>,

    // Communication channels
    tx_from_server_message: Option<UnboundedSender<psyche_coordinator::Coordinator<ClientId>>>,
    rx_to_server_message: Option<UnboundedReceiver<ToSend>>,

    // Stuff needed for simulations
    pub router: Option<Arc<Router>>,
    pub gossip_metrics: Option<Arc<GossipMetrics>>,
    pub blob_metrics: Option<Arc<BlobsMetrics>>,
    pub endpoint: Option<Endpoint>,

    // checks
    backend: Option<Backend>,
    state_options: Option<RunInitConfig<ClientId, ClientId>>,
    allowlist: Option<allowlist::AllowDynamic>,
    p2p: Option<NC>,
}

#[derive(Debug)]
pub struct AppParams {
    pub cancel: CancellationToken,
    pub identity_secret_key: SecretKey,
    pub server_addr: String,
    pub tx_tui_state: Option<Sender<TabsData>>,
    pub run_id: String,
    pub data_parallelism: usize,
    pub tensor_parallelism: usize,
    pub micro_batch_size: usize,
    pub write_gradients_dir: Option<PathBuf>,
    pub p2p_port: Option<u16>,
    pub p2p_interface: Option<String>,
    pub eval_tasks: Vec<psyche_eval::Task>,
    pub eval_task_max_docs: Option<usize>,
    pub prompt_task: bool,
    pub checkpoint_upload_info: Option<CheckpointConfig>,
    pub hub_read_token: Option<String>,
    pub hub_max_concurrent_downloads: usize,
    pub wandb_info: Option<WandBInfo>,
    pub optim_stats: Option<u32>,
    pub grad_accum_in_fp32: bool,
    pub dummy_training_delay_secs: Option<u64>,
    pub discovery_mode: DiscoveryMode,
    pub max_concurrent_parameter_requests: usize,
    pub max_concurrent_downloads: usize,
    pub metrics_local_port: Option<u16>,
    pub sim_endpoint: Option<Endpoint>,
}

impl App {
    /// Creates an application instance (consumes params).
    pub async fn new(params: &AppParams) -> Result<Self> {
        let cancel = params.cancel.clone();
        let tx_tui_state = params.tx_tui_state.clone();
        let run_id = params.run_id.clone();
        let metrics = Arc::new(ClientMetrics::new(params.metrics_local_port));
        let allowlist = allowlist::AllowDynamic::new();

        // let server_conn =
        //     TcpClient::<ClientId, ClientToServerMessage, ServerToClientMessage>::connect(
        //         &params.server_addr,
        //         params.identity_secret_key.public().into(),
        //         params.identity_secret_key.clone(),
        //     )
        //     .await?;

        let mut app = Self {
            cancel,
            tx_tui_state,
            run_id,
            coordinator_state: Coordinator::zeroed(),
            update_tui_interval: interval(Duration::from_millis(150)),
            // server_conn: Some(server_conn),
            server_conn: None,
            client: None,
            metrics: Some(metrics.clone()),
            tx_from_server_message: None,
            rx_to_server_message: None,
            router: None,
            gossip_metrics: None,
            blob_metrics: None,
            backend: None,
            state_options: None,
            allowlist: None,
            p2p: None,
            endpoint: None,
        };

        let (tx_from_server_message, rx_from_server_message) = mpsc::unbounded_channel();
        let (tx_to_server_message, mut rx_to_server_message) = mpsc::unbounded_channel();

        // Perform sanity checks immediately
        app.validate_configuration(&params).await?;

        let backend = Backend {
            allowlist: allowlist.clone(),
            rx: rx_from_server_message,
            tx: tx_to_server_message,
        };

        // // Join the server
        // app.join_server().await?;

        // Initialize the client (this consumes params)
        // app.initialize_client(params, rx_from_server_message, tx_to_server_message)
        //     .await?;
        //
        let p2p = NC::init(
            &params.run_id,
            params.p2p_port,
            params.p2p_interface.clone(),
            RelayMode::Custom(psyche_relay_map()),
            params.discovery_mode,
            vec![],
            Some(params.identity_secret_key.clone()),
            allowlist.clone(),
            params.max_concurrent_downloads,
            metrics.clone(),
            params.sim_endpoint.clone(),
        )
        .await?;

        // Store channels in the struct so `run()` can use them later
        app.tx_from_server_message = Some(tx_from_server_message);
        app.rx_to_server_message = Some(rx_to_server_message);
        app.gossip_metrics = Some(p2p.gossip.metrics().clone());
        app.blob_metrics = Some(p2p.blobs.metrics().clone());

        app.router = Some(p2p.router().clone());
        app.backend = Some(backend);
        app.endpoint = Some(p2p.endpoint.clone());
        app.p2p = Some(p2p);
        app.allowlist = Some(allowlist);
        Ok(app)
    }

    /// Runs the already-created app (doesn't need params anymore).
    pub async fn run(mut self, params: AppParams) -> Result<()> {
        debug!("Starting app loop");

        let server_conn =
            TcpClient::<ClientId, ClientToServerMessage, ServerToClientMessage>::connect(
                &params.server_addr,
                params.identity_secret_key.public().into(),
                params.identity_secret_key.clone(),
            )
            .await?;
        self.server_conn = Some(server_conn);
        self.p2p
            .as_mut()
            .unwrap()
            .run(self.allowlist.clone().unwrap(), params.run_id.clone())
            .await?;

        self.join_server().await?;

        // Set up communication channels
        // let (tx_from_server_message, rx_from_server_message) = mpsc::unbounded_channel();
        // let (tx_to_server_message, mut rx_to_server_message) = mpsc::unbounded_channel();

        // Initialize the client (this consumes params)
        self.initialize_client(params).await?;

        let mut rx_to_server_message = self.rx_to_server_message.take().unwrap();
        let tx_from_server_message = self.tx_from_server_message.clone().unwrap();
        self.run_main_loop(tx_from_server_message, &mut rx_to_server_message)
            .await
    }

    pub fn endpoint(&self) -> Option<Endpoint> {
        let endpoint = self.p2p.as_ref().unwrap().endpoint.clone();
        Some(endpoint)
    }

    async fn validate_configuration(&self, params: &AppParams) -> Result<()> {
        if let Some(checkpoint_config) = &params.checkpoint_upload_info {
            if let Some(hub_upload) = &checkpoint_config.hub_upload {
                let api = hf_hub::api::tokio::ApiBuilder::new()
                    .with_token(Some(hub_upload.hub_token.clone()))
                    .build()?;
                let repo_api = api.repo(Repo::new(
                    hub_upload.hub_repo.clone(),
                    hf_hub::RepoType::Model,
                ));
                if !repo_api.is_writable().await {
                    anyhow::bail!(
                        "Checkpoint upload repo {} is not writable with the passed API key.",
                        hub_upload.hub_repo
                    )
                }
            }
        }
        Ok(())
    }

    async fn join_server(&mut self) -> Result<()> {
        let server_conn = self
            .server_conn
            .as_mut()
            .ok_or_else(|| Error::msg("Server connection not initialized"))?;

        server_conn
            .send(ClientToServerMessage::Join {
                run_id: self.run_id.clone(),
            })
            .await?;

        Ok(())
    }

    async fn initialize_client(&mut self, params: AppParams) -> Result<()> {
        let allowlist = allowlist::AllowDynamic::new();

        // let p2p = NC::init(
        //     &params.run_id,
        //     params.p2p_port,
        //     params.p2p_interface,
        //     RelayMode::Custom(psyche_relay_map()),
        //     params.discovery_mode,
        //     vec![],
        //     Some(params.identity_secret_key.clone()),
        //     allowlist.clone(),
        //     params.max_concurrent_downloads,
        //     self.metrics.as_ref().unwrap().clone(),
        //     params.sim_endpoint,
        // )
        // .await?;

        let state_options = RunInitConfig {
            data_parallelism: params.data_parallelism,
            tensor_parallelism: params.tensor_parallelism,
            micro_batch_size: params.micro_batch_size,
            write_gradients_dir: params.write_gradients_dir,
            eval_tasks: params.eval_tasks, // Move ownership
            eval_task_max_docs: params.eval_task_max_docs,
            prompt_task: params.prompt_task,
            checkpoint_config: params.checkpoint_upload_info, // Move ownership
            hub_read_token: params.hub_read_token,            // Move ownership
            hub_max_concurrent_downloads: params.hub_max_concurrent_downloads,
            wandb_info: params.wandb_info, // Move ownership
            identity: params.identity_secret_key.public().into(),
            network_identity: params.identity_secret_key.public().into(),
            private_key: params.identity_secret_key,
            optim_stats_every_n_steps: params.optim_stats,
            grad_accum_in_fp32: params.grad_accum_in_fp32,
            dummy_training_delay_secs: params.dummy_training_delay_secs,
            max_concurrent_parameter_requests: params.max_concurrent_parameter_requests,
        };

        // let backend = Backend {
        //     allowlist: allowlist.clone(),
        //     rx: rx_from_server_message,
        //     tx: tx_to_server_message,
        // };

        let p2p = self.p2p.take().unwrap();
        let backend = self.backend.take().unwrap();

        let blob_metrics = p2p.blobs.metrics().clone();
        let gossip_metrics = p2p.gossip.metrics().clone();
        let router = p2p.router().clone();
        let client = Client::new(
            backend,
            allowlist,
            p2p,
            state_options,
            self.metrics.as_ref().unwrap().clone(),
        );

        // self.blob_metrics = Some(blob_metrics);
        // self.gossip_metrics = Some(gossip_metrics);
        self.router = Some(router);
        self.client = Some(client);
        Ok(())
    }

    async fn run_main_loop(
        mut self,
        tx_from_server_message: mpsc::UnboundedSender<Coordinator<ClientId>>,
        rx_to_server_message: &mut mpsc::UnboundedReceiver<ToSend>,
    ) -> Result<()> {
        loop {
            select! {
                _ = self.cancel.cancelled() => {
                   break;
                }
                message = self.server_conn.as_mut().unwrap().receive() => {
                    self.on_server_message(message?, &tx_from_server_message).await;
                }
                _ = self.update_tui_interval.tick() => {
                    let client = self.client.as_mut()
                        .ok_or_else(|| Error::msg("Client not initialized"))?;
                    let (client_tui_state, network_tui_state) = client.tui_states().await;
                    self.update_tui(client_tui_state, network_tui_state).await?;
                }
                Some(to_send) = rx_to_server_message.recv() => {
                    let server_conn = self.server_conn.as_mut()
                        .ok_or_else(|| Error::msg("Server connection not initialized"))?;
                    match to_send {
                        ToSend::Witness(witness) => server_conn.send(ClientToServerMessage::Witness(witness)).await?,
                        ToSend::HealthCheck(health_checks) => server_conn.send(ClientToServerMessage::HealthCheck(health_checks)).await?,
                        ToSend::Checkpoint(checkpoint) => server_conn.send(ClientToServerMessage::Checkpoint(checkpoint)).await?,
                    };
                }
            }
        }
        Ok(())
    }

    async fn update_tui(
        &mut self,
        client_tui_state: ClientTUIState,
        network_tui_state: NetworkTUIState,
    ) -> Result<()> {
        if let Some(tx_tui_state) = &self.tx_tui_state {
            let states = (
                client_tui_state,
                (&self.coordinator_state).into(),
                network_tui_state,
                Default::default(),
            );
            tx_tui_state.send(states).await?;
        }
        Ok(())
    }

    async fn on_server_message(
        &mut self,
        message: ServerToClientMessage,
        tx: &mpsc::UnboundedSender<Coordinator<ClientId>>,
    ) {
        match message {
            ServerToClientMessage::Coordinator(state) => {
                self.coordinator_state = *state;
                let _ = tx.send(*state);
            }
        }
    }
}

// Usage becomes much simpler:
// let app = App::new(params.clone()); // or move some fields to App::new()
// app.run(params).await?;
