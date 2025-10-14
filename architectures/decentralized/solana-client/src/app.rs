use crate::{backend::SolanaBackend, network_identity::NetworkIdentity};

use anchor_client::{
    Cluster,
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{Keypair, Signer},
    },
};
use anyhow::{Result, anyhow};
use psyche_client::{
    CheckpointConfig, Client, ClientTUI, ClientTUIState, NC, RunInitConfig, WandBInfo,
};
use psyche_coordinator::{ClientState, Coordinator, CoordinatorError, RunState};
use psyche_metrics::ClientMetrics;

use psyche_modeling::Devices;
use psyche_network::{DiscoveryMode, NetworkTUIState, NetworkTui, SecretKey, allowlist};
use psyche_tui::{CustomWidget, TabbedWidget, logging::LoggerWidget};
use psyche_watcher::CoordinatorTui;
use rand::{Rng, RngCore, thread_rng};
use std::{path::PathBuf, time::Duration};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    select,
    sync::mpsc::Sender,
    time::{Interval, MissedTickBehavior, interval},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

pub(super) type Tabs = TabbedWidget<(ClientTUI, CoordinatorTui, NetworkTui, LoggerWidget)>;
pub const TAB_NAMES: [&str; 4] = ["Client", "Coordinator", "Network", "Logger"];
type TabsData = <Tabs as CustomWidget>::Data;

pub struct App {
    run_id: String,
    cluster: Cluster,
    backup_clusters: Vec<Cluster>,
    tick_check_interval: Interval,
    cancel: CancellationToken,
    update_tui_interval: Interval,
    tx_tui_state: Option<Sender<TabsData>>,
    authorizer: Option<Pubkey>,
    metrics: Arc<ClientMetrics>,
    allowlist: allowlist::AllowDynamic,
    p2p: NC,
    state_options: RunInitConfig<psyche_solana_coordinator::ClientId, NetworkIdentity>,
}

pub struct AppBuilder(AppParams);

pub struct AppParams {
    pub cancel: CancellationToken,
    pub identity_secret_key: SecretKey,
    pub wallet_keypair: Arc<Keypair>,
    pub cluster: Cluster,
    pub backup_clusters: Vec<Cluster>,
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
    pub max_concurrent_parameter_requests: usize,
    pub authorizer: Option<Pubkey>,
    pub metrics_local_port: Option<u16>,
    pub device: Devices,
    pub sidecar_port: Option<u16>,
}

impl AppBuilder {
    pub fn new(params: AppParams) -> Self {
        Self(params)
    }

    pub async fn build(self) -> Result<App> {
        let p = self.0;
        let identity = psyche_solana_coordinator::ClientId::new(
            p.wallet_keypair.pubkey(),
            *p.identity_secret_key.public().as_bytes(),
        );

        let metrics = Arc::new(ClientMetrics::new(p.metrics_local_port));

        let allowlist = allowlist::AllowDynamic::new();

        let p2p = NC::init(
            &p.run_id,
            p.p2p_port,
            p.p2p_interface,
            DiscoveryMode::N0,
            vec![],
            Some(p.identity_secret_key.clone()),
            allowlist.clone(),
            metrics.clone(),
        )
        .await?;

        let state_options: RunInitConfig<psyche_solana_coordinator::ClientId, NetworkIdentity> =
            RunInitConfig {
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
                identity,
                network_identity: identity.into(),
                private_key: (p.wallet_keypair.clone(), p.identity_secret_key),
                optim_stats_every_n_steps: p.optim_stats,
                grad_accum_in_fp32: p.grad_accum_in_fp32,
                dummy_training_delay_secs: p.dummy_training_delay_secs,
                max_concurrent_parameter_requests: p.max_concurrent_parameter_requests,
                device: p.device,
                sidecar_port: p.sidecar_port,
            };
        let app = App {
            run_id: p.run_id.clone(),
            cluster: p.cluster,
            backup_clusters: p.backup_clusters,
            tick_check_interval: {
                let mut interval = interval(Duration::from_millis(500));
                interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
                interval
            },
            cancel: p.cancel,
            tx_tui_state: p.tx_tui_state,
            update_tui_interval: interval(Duration::from_millis(150)),
            authorizer: p.authorizer,
            allowlist,
            metrics,
            p2p,
            state_options,
        };
        Ok(app)
    }
}

impl App {
    pub async fn run(mut self) -> Result<()> {
        let backend = SolanaBackend::new(
            self.cluster.clone(),
            self.backup_clusters.clone(),
            self.state_options.private_key.0.clone(),
            CommitmentConfig::confirmed(),
        )?;
        let coordinator_instance =
            psyche_solana_coordinator::find_coordinator_instance(&self.run_id);
        let coordinator_instance_state = backend
            .get_coordinator_instance(&coordinator_instance)
            .await?;

        // Check client version compatibility before joining
        let client_version =
            std::env::var("PSYCHE_CLIENT_VERSION").unwrap_or_else(|_| "latest".to_string());
        if client_version != coordinator_instance_state.client_version {
            tracing::error!(
                client_version = %client_version,
                coordinator_version = %coordinator_instance_state.client_version,
                "Version mismatch detected. Client version does not match coordinator version."
            );
            std::process::exit(10);
        }
        info!(
            client_version = %client_version,
            coordinator_version = %coordinator_instance_state.client_version,
            "Version check passed"
        );

        let coordinator_account = coordinator_instance_state.coordinator_account;

        let backend_runner = backend
            .start(self.run_id.clone(), coordinator_account)
            .await?;

        let backend = Arc::new(SolanaBackend::new(
            self.cluster.clone(),
            self.backup_clusters.clone(),
            self.state_options.private_key.0.clone(),
            CommitmentConfig::confirmed(),
        )?);
        let signer = self.state_options.private_key.0.pubkey();
        let p2p_identity = self.state_options.private_key.1.public();

        let start_coordinator_state = backend
            .get_coordinator_account(&coordinator_account)
            .await?
            .state
            .coordinator;

        let mut joined_run_this_epoch = None;
        let mut ever_joined_run = false;

        // if we're already in "WaitingForMembers" we won't get an update saying that
        // (subscription is on change), so check if it's in that state right at boot
        // and join the run if so
        if start_coordinator_state.run_state == RunState::WaitingForMembers {
            let join_signature = backend
                .join_run(
                    coordinator_instance,
                    coordinator_account,
                    psyche_solana_coordinator::ClientId {
                        signer,
                        p2p_identity: *p2p_identity.as_bytes(),
                    },
                    self.authorizer,
                )
                .await?;
            info!(
                run_id = self.run_id,
                from = %signer,
                tx = %join_signature,
                "Joined run",
            );
            joined_run_this_epoch = Some(join_signature);
            ever_joined_run = true;
        } else {
            info!("Waiting for the current epoch to end before joining");
        }

        // Update the latest update after joining the run to advance the state.
        let coordinator_state = backend
            .get_coordinator_account(&coordinator_account)
            .await?
            .state;

        let mut latest_update = coordinator_state.coordinator;
        let mut updates = backend_runner.updates();
        let mut client = Client::new(
            backend_runner,
            self.allowlist,
            self.p2p,
            self.state_options,
            self.metrics,
        );

        let id = psyche_solana_coordinator::ClientId {
            signer,
            p2p_identity: *p2p_identity.as_bytes(),
        };

        loop {
            select! {
                _ = self.cancel.cancelled() => {
                   break;
                }
                _ = self.update_tui_interval.tick() => {
                    let (client_tui_state, network_tui_state) = client.tui_states().await;
                    Self::update_tui(&self.tx_tui_state, client_tui_state, &latest_update, network_tui_state).await?;
                }
                _ = self.tick_check_interval.tick() => {
                    let mut ticked = latest_update;
                    let timestamp = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();

                    let coordinator_state_in_waiting_for_members = if ticked.run_state == RunState::WaitingForMembers {
                        Some(backend
                            .get_coordinator_account(&coordinator_account)
                            .await?
                            .state)
                    } else {
                        None
                    };

                    let pending_clients_ids = coordinator_state_in_waiting_for_members
                        .as_ref()
                        .map(|state| state.clients_state.get_active_clients_ids());

                    match ticked.tick(pending_clients_ids, timestamp, rand::thread_rng().next_u64()) {
                        Ok(_) => {
                            if ticked.run_state != latest_update.run_state {
                                // to avoid *everyone* sending a tick, we probabilisticly send it
                                // targeting having two clients send it per interval
                                let send_tick = match ticked.epoch_state.clients.len() {
                                    0..=2 => true,
                                    len => { let rand: f32 = thread_rng().r#gen();
                                        rand <= 2.0 / len as f32
                                    }
                                };
                                if send_tick {
                                    backend.send_tick(coordinator_instance, coordinator_account);
                                }
                            }
                        }
                        Err(CoordinatorError::Halted) => {}, // don't print anything when halted. it's an "error" but no need to spam logs
                        Err(err) => debug!("Tick simulation error: {err}")
                    };
                }
                update = updates.recv() => {
                    latest_update = update?;
                    match latest_update.run_state {
                        RunState::WaitingForMembers => {
                            if joined_run_this_epoch.is_none() {
                                let join_signature = backend
                                    .join_run(
                                        coordinator_instance,
                                        coordinator_account,
                                        id,
                                        self.authorizer,
                                    )
                                    .await?;
                                info!(
                                    run_id = self.run_id,
                                    from = %signer,
                                    tx = %join_signature,
                                    "Joined run",
                                );
                                joined_run_this_epoch = Some(join_signature);
                                ever_joined_run = true;
                            }
                        }
                        _ => {
                            if ever_joined_run {
                                let err = if latest_update.halted() {
                                    Err(anyhow!("{}", latest_update.run_state))
                                } else {
                                    let me = latest_update.epoch_state.clients.iter().find(|x| x.id == id);
                                    match me {
                                        Some(me) => if me.state != ClientState::Healthy {
                                            tracing::error!(id = %id, state = %me.state, "Coordinator says we're unhealthy, exiting");
                                            Err(anyhow!("{}", me.state))
                                        } else {
                                            Ok(())
                                        }
                                        None => {
                                            tracing::error!(id = %id, "Coordinator did not select us for the round, exiting");
                                            Err(anyhow!("Not a participant"))
                                        }
                                    }
                                };
                                if let Err(err) = err {
                                    client.shutdown();
                                    let _ = client.finished().await;
                                    return Err(err);
                                }
                            }
                            joined_run_this_epoch = None;
                        }
                    }
                }
                res = client.finished() => {
                    res??;
                }

            }
        }

        Ok(())
    }

    async fn update_tui(
        tx_tui_state: &Option<Sender<<Tabs as CustomWidget>::Data>>,
        client_tui_state: ClientTUIState,
        coordinator_state: &Coordinator<psyche_solana_coordinator::ClientId>,
        network_tui_state: NetworkTUIState,
    ) -> Result<()> {
        if let Some(tx_tui_state) = &tx_tui_state {
            let states = (
                client_tui_state,
                coordinator_state.into(),
                network_tui_state,
                Default::default(),
            );
            tx_tui_state.send(states).await?;
        }
        Ok(())
    }
}
