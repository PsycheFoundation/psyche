use anyhow::{Error, Result};
use bytemuck::Zeroable;
use hf_hub::Repo;
use psyche_centralized_shared::{ClientId, ClientToServerMessage, ServerToClientMessage};
use psyche_client::{
    CheckpointConfig, Client, ClientTUI, ClientTUIState, NC, RunInitConfig, WandBInfo,
};
use psyche_coordinator::{Coordinator, HealthChecks, model};
use psyche_metrics::ClientMetrics;
use psyche_modeling::Devices;
use psyche_network::{
    AuthenticatableIdentity, DiscoveryMode, EndpointId, NetworkTUIState, NetworkTui, SecretKey,
    TcpClient, allowlist,
};
use psyche_tui::logging::LoggerWidget;
use psyche_tui::{CustomWidget, TabbedWidget};
use psyche_watcher::{Backend as WatcherBackend, CoordinatorTui, OpportunisticData};
use std::sync::Arc;
use std::{path::PathBuf, time::Duration};
use tokio::sync::mpsc::Sender;
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
                .map(|c| EndpointId::from_bytes(c.id.get_p2p_public_key()).unwrap()),
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

pub struct App {
    run_id: String,
    cancel: CancellationToken,
    update_tui_interval: Interval,
    tx_tui_state: Option<Sender<TabsData>>,
    coordinator_state: Coordinator<ClientId>,
    server_conn: TcpClient<ClientId, ClientToServerMessage, ServerToClientMessage>,

    metrics: Arc<ClientMetrics>,
}

pub struct AppBuilder(AppParams);

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
    pub metrics_local_port: Option<u16>,
    pub device: Devices,
    pub sidecar_port: Option<u16>,
}

impl AppBuilder {
    pub fn new(params: AppParams) -> Self {
        Self(params)
    }

    pub async fn build(
        self,
    ) -> Result<(
        App,
        allowlist::AllowDynamic,
        NC,
        RunInitConfig<ClientId, ClientId>,
    )> {
        let p = self.0;

        let metrics = Arc::new(ClientMetrics::new(p.metrics_local_port));
        let server_conn =
            TcpClient::<ClientId, ClientToServerMessage, ServerToClientMessage>::connect(
                &p.server_addr,
                p.identity_secret_key.public().into(),
                p.identity_secret_key.clone(),
            )
            .await?;

        let allowlist = allowlist::AllowDynamic::new();

        let p2p = NC::init(
            &p.run_id,
            p.p2p_port,
            p.p2p_interface,
            p.discovery_mode,
            vec![],
            Some(p.identity_secret_key.clone()),
            allowlist.clone(),
            metrics.clone(),
            Some(p.cancel.clone()),
        )
        .await?;

        let state_options: RunInitConfig<ClientId, ClientId> = RunInitConfig {
            data_parallelism: p.data_parallelism,
            tensor_parallelism: p.tensor_parallelism,
            micro_batch_size: p.micro_batch_size,
            write_gradients_dir: p.write_gradients_dir,
            eval_tasks: p.eval_tasks,
            eval_task_max_docs: p.eval_task_max_docs,
            prompt_task: p.prompt_task,
            checkpoint_config: p.checkpoint_upload_info,
            hub_read_token: p.hub_read_token,
            hub_max_concurrent_downloads: p.hub_max_concurrent_downloads,
            wandb_info: p.wandb_info,
            identity: p.identity_secret_key.public().into(),
            network_identity: p.identity_secret_key.public().into(),
            private_key: p.identity_secret_key,
            optim_stats_every_n_steps: p.optim_stats,
            grad_accum_in_fp32: p.grad_accum_in_fp32,
            dummy_training_delay_secs: p.dummy_training_delay_secs,
            max_concurrent_parameter_requests: p.max_concurrent_parameter_requests,
            device: p.device,
            sidecar_port: p.sidecar_port,
        };
        let app = App {
            cancel: p.cancel,
            tx_tui_state: p.tx_tui_state,
            update_tui_interval: interval(Duration::from_millis(150)),
            coordinator_state: Coordinator::zeroed(),
            server_conn,
            run_id: p.run_id,
            metrics,
        };
        Ok((app, allowlist, p2p, state_options))
    }
}

impl App {
    pub async fn run(
        &mut self,
        allowlist: allowlist::AllowDynamic,
        p2p: NC,
        state_options: RunInitConfig<ClientId, ClientId>,
    ) -> Result<()> {
        // sanity checks
        if let Some(checkpoint_config) = &state_options.checkpoint_config {
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

        self.server_conn
            .send(ClientToServerMessage::Join {
                run_id: self.run_id.clone(),
            })
            .await?;

        let (tx_from_server_message, rx_from_server_message) = mpsc::unbounded_channel();
        let (tx_to_server_message, mut rx_to_server_message) = mpsc::unbounded_channel();
        let mut client = Client::new(
            Backend {
                allowlist: allowlist.clone(),
                rx: rx_from_server_message,
                tx: tx_to_server_message,
            },
            allowlist,
            p2p,
            state_options,
            self.metrics.clone(),
        );

        debug!("Starting app loop");
        loop {
            select! {
                _ = self.cancel.cancelled() => {
                   break;
                }
                message = self.server_conn.receive() => {
                    self.on_server_message(message?, &tx_from_server_message).await;
                }
                _ = self.update_tui_interval.tick() => {
                    let (client_tui_state, network_tui_state) = client.tui_states().await;
                    self.update_tui(client_tui_state, network_tui_state).await?;
                }
                res = client.finished() => {
                    res??;
                }
                Some(to_send) = rx_to_server_message.recv() => {
                    match to_send {
                        ToSend::Witness(witness) => self.server_conn.send(ClientToServerMessage::Witness(witness)).await?,
                        ToSend::HealthCheck(health_checks) => self.server_conn.send(ClientToServerMessage::HealthCheck(health_checks)).await?,
                        ToSend::Checkpoint(checkpoint) => self.server_conn.send(ClientToServerMessage::Checkpoint(checkpoint)).await?,
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
