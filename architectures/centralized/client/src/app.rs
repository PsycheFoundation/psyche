use anyhow::{Error, Result};
use bytemuck::Zeroable;
use hf_hub::Repo;
use psyche_centralized_shared::{ClientId, ClientToServerMessage, ServerToClientMessage};
use psyche_client::HubUploadInfo;
use psyche_client::UploadInfo;
use psyche_client::{
    Client, ClientTUI, ClientTUIState, NC, RunInitConfig, TrainArgs, read_identity_secret_key,
};
use psyche_coordinator::{Coordinator, HealthChecks, model};
use psyche_event_sourcing::event;
use psyche_event_sourcing::events::RpcCallType;
use psyche_metrics::ClientMetrics;
use psyche_network::{
    AuthenticatableIdentity, EndpointId, NetworkTUIState, NetworkTui, SecretKey, TcpClient,
    allowlist,
};
use psyche_tui::logging::LoggerWidget;
use psyche_tui::{CustomWidget, TabbedWidget};
use psyche_watcher::{Backend as WatcherBackend, CoordinatorTui, OpportunisticData};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::time::interval;
use tokio::{select, sync::mpsc, time::Interval};
use tokio_util::sync::CancellationToken;
use tracing::debug;

pub(super) type Tabs = TabbedWidget<(ClientTUI, CoordinatorTui, NetworkTui, LoggerWidget)>;
pub const TAB_NAMES: [&str; 4] = ["Client", "Coordinator", "Network", "Logger"];
pub type TabsData = <Tabs as CustomWidget>::Data;

pub enum ToSend {
    Witness(Box<OpportunisticData>),
    HealthCheck(HealthChecks<ClientId>),
    Checkpoint(model::Checkpoint),
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

    async fn send_checkpoint(&mut self, checkpoint: model::Checkpoint) -> Result<()> {
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

pub async fn build_app(
    cancel: CancellationToken,
    server_addr: String,
    tx_tui_state: Option<Sender<TabsData>>,
    p: TrainArgs,
) -> Result<(
    App,
    allowlist::AllowDynamic,
    NC,
    RunInitConfig<ClientId, ClientId>,
)> {
    let metrics = Arc::new(ClientMetrics::new(
        p.metrics_local_port,
        Some(Duration::from_secs(30)),
    ));
    let identity_secret_key = read_identity_secret_key(p.identity_secret_key_path.as_ref())?
        .unwrap_or_else(|| SecretKey::generate(&mut rand::rng()));
    let server_conn = TcpClient::<ClientId, ClientToServerMessage, ServerToClientMessage>::connect(
        &server_addr,
        identity_secret_key.public().into(),
        identity_secret_key.clone(),
    )
    .await?;

    let hub_read_token = std::env::var("HF_TOKEN").ok();
    let eval_tasks = p.eval_tasks()?;
    let checkpoint_config = p.checkpoint_config()?;
    let wandb_info = p.wandb_info(format!(
        "{}-{}",
        p.run_id.clone(),
        identity_secret_key.public().fmt_short()
    ))?;

    let allowlist = allowlist::AllowDynamic::new();

    let p2p = NC::init(
        &p.run_id,
        p.bind_p2p_port,
        p.bind_p2p_interface,
        p.iroh_discovery,
        p.iroh_relay,
        vec![],
        Some(identity_secret_key.clone()),
        allowlist.clone(),
        metrics.clone(),
        Some(cancel.clone()),
    )
    .await?;

    let state_options: RunInitConfig<ClientId, ClientId> = RunInitConfig {
        data_parallelism: p.data_parallelism,
        tensor_parallelism: p.tensor_parallelism,
        micro_batch_size: p.micro_batch_size,
        write_gradients_dir: p.write_gradients_dir,
        eval_tasks,
        eval_task_max_docs: p.eval_task_max_docs,
        prompt_task: p.prompt_task,
        checkpoint_config,
        hub_read_token,
        hub_max_concurrent_downloads: p.hub_max_concurrent_downloads,
        wandb_info,
        identity: identity_secret_key.public().into(),
        network_identity: identity_secret_key.public().into(),
        private_key: identity_secret_key,
        optim_stats_every_n_steps: p.optim_stats_steps,
        grad_accum_in_fp32: p.grad_accum_in_fp32,
        dummy_training_delay_secs: p.dummy_training_delay_secs,
        max_concurrent_parameter_requests: p.max_concurrent_parameter_requests,
        device: p.device,
        sidecar_port: p.sidecar_port,
    };
    let app = App {
        cancel,
        tx_tui_state,
        update_tui_interval: interval(Duration::from_millis(150)),
        coordinator_state: Coordinator::zeroed(),
        server_conn,
        run_id: p.run_id,
        metrics,
    };
    Ok((app, allowlist, p2p, state_options))
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
            if let Some(UploadInfo::Hub(HubUploadInfo {
                hub_repo,
                hub_token,
            })) = &checkpoint_config.upload_info
            {
                let api = hf_hub::api::tokio::ApiBuilder::new()
                    .with_token(Some(hub_token.clone()))
                    .build()?;
                let repo_api = api.repo(Repo::new(hub_repo.clone(), hf_hub::RepoType::Model));
                if !repo_api.is_writable().await {
                    anyhow::bail!(
                        "Checkpoint upload repo {} is not writable with the passed API key.",
                        hub_repo
                    )
                }
            }
        }

        event!(coordinator::RpcCallSubmitted {
            call_type: RpcCallType::Join
        });
        match self
            .server_conn
            .send(ClientToServerMessage::Join {
                run_id: self.run_id.clone(),
            })
            .await
        {
            Ok(()) => event!(coordinator::RpcCallResult {
                call_type: RpcCallType::Join,
                success: true,
                error_string: None
            }),
            Err(e) => {
                event!(coordinator::RpcCallResult {
                    call_type: RpcCallType::Join,
                    success: false,
                    error_string: Some(e.to_string())
                });
                return Err(e);
            }
        }

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
                    let (msg, call_type) = match to_send {
                        ToSend::Witness(ref w) => {
                            let ct = match **w {
                                OpportunisticData::WitnessStep(..) => RpcCallType::Witness,
                                OpportunisticData::WarmupStep(..) => RpcCallType::WarmupWitness,
                            };
                            (ClientToServerMessage::Witness(match to_send { ToSend::Witness(w) => w, _ => unreachable!() }), ct)
                        }
                        ToSend::HealthCheck(hc) => (ClientToServerMessage::HealthCheck(hc), RpcCallType::HealthCheck),
                        ToSend::Checkpoint(cp) => (ClientToServerMessage::Checkpoint(cp), RpcCallType::Checkpoint),
                    };
                    event!(coordinator::RpcCallSubmitted { call_type });
                    match self.server_conn.send(msg).await {
                        Ok(()) => event!(coordinator::RpcCallResult { call_type, success: true, error_string: None }),
                        Err(e) => {
                            event!(coordinator::RpcCallResult { call_type, success: false, error_string: Some(e.to_string()) });
                            return Err(e);
                        }
                    }
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
