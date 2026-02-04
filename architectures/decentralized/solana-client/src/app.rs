use crate::network_identity::NetworkIdentity;
use psyche_solana_rpc::SolanaBackend;

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
    Client, ClientTUI, ClientTUIState, NC, RunInitConfig, TrainArgs, read_identity_secret_key,
};
use psyche_coordinator::{ClientState, Coordinator, CoordinatorError, RunState};
use psyche_core::sha256;
use psyche_metrics::ClientMetrics;

use psyche_network::{DiscoveryMode, NetworkTUIState, NetworkTui, SecretKey, allowlist};
use psyche_tui::{CustomWidget, TabbedWidget, logging::LoggerWidget};
use psyche_watcher::CoordinatorTui;
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::time::Duration;
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
use tracing::{debug, info, warn};

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

pub struct AppParams {
    pub cancel: CancellationToken,
    pub wallet_keypair: Arc<Keypair>,
    pub cluster: Cluster,
    pub backup_clusters: Vec<Cluster>,
    pub tx_tui_state: Option<Sender<TabsData>>,
    pub authorizer: Option<Pubkey>,
    pub train_args: TrainArgs,
}

pub async fn build_app(
    AppParams {
        cancel,
        wallet_keypair,
        cluster,
        backup_clusters,
        tx_tui_state,
        authorizer,
        train_args: p,
    }: AppParams,
) -> Result<App> {
    let identity_secret_key: SecretKey =
        read_identity_secret_key(p.identity_secret_key_path.as_ref())?
            // Iroh key should be deterministically derived from Solana key
            .unwrap_or_else(|| {
                let mut rng = ChaCha8Rng::from_seed(sha256(wallet_keypair.secret().as_bytes()));
                SecretKey::generate(&mut rng)
            });
    let identity = psyche_solana_coordinator::ClientId::new(
        wallet_keypair.pubkey(),
        *identity_secret_key.public().as_bytes(),
    );

    let eval_tasks = p.eval_tasks()?;
    let hub_read_token = std::env::var("HF_TOKEN").ok();
    let checkpoint_config = p.checkpoint_config()?;

    let solana_pubkey = wallet_keypair.pubkey();
    let wandb_info = p.wandb_info(format!("{}-{solana_pubkey}", p.run_id))?;

    let metrics = Arc::new(ClientMetrics::new(
        p.metrics_local_port,
        Some(Duration::from_secs(30)),
    ));

    let allowlist = allowlist::AllowDynamic::new();

    let p2p = NC::init(
        &p.run_id,
        p.bind_p2p_port,
        p.bind_p2p_interface,
        DiscoveryMode::N0,
        p.iroh_relay,
        vec![],
        Some(identity_secret_key.clone()),
        allowlist.clone(),
        metrics.clone(),
        Some(cancel.clone()),
    )
    .await?;

    let state_options: RunInitConfig<psyche_solana_coordinator::ClientId, NetworkIdentity> =
        RunInitConfig {
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
            identity,
            network_identity: identity.into(),
            private_key: (wallet_keypair.clone(), identity_secret_key),
            optim_stats_every_n_steps: p.optim_stats_steps,
            grad_accum_in_fp32: p.grad_accum_in_fp32,
            dummy_training_delay_secs: p.dummy_training_delay_secs,
            max_concurrent_parameter_requests: p.max_concurrent_parameter_requests,
            device: p.device,
            sidecar_port: p.sidecar_port,
        };
    let app = App {
        run_id: p.run_id.clone(),
        cluster,
        backup_clusters,
        tick_check_interval: {
            let mut interval = interval(Duration::from_millis(500));
            interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
            interval
        },
        cancel,
        tx_tui_state,
        update_tui_interval: interval(Duration::from_millis(150)),
        authorizer,
        allowlist,
        metrics,
        p2p,
        state_options,
    };
    Ok(app)
}

impl App {
    pub async fn run(mut self) -> Result<()> {
        let backend = SolanaBackend::new(
            self.cluster.clone(),
            self.backup_clusters.clone(),
            self.state_options.private_key.0.clone(),
            CommitmentConfig::confirmed(),
        )?;
        let coordinator_instance_pubkey =
            psyche_solana_coordinator::find_coordinator_instance(&self.run_id);
        let coordinator_instance = backend
            .get_coordinator_instance(&coordinator_instance_pubkey)
            .await?;

        let coordinator_account = coordinator_instance.coordinator_account;
        let coordinator_account_pubkey = coordinator_instance.coordinator_account;
        let coordinator_client_version = String::from(
            &backend
                .get_coordinator_account(&coordinator_account_pubkey)
                .await?
                .state
                .client_version,
        );

        // Check client version compatibility before joining
        let client_version = std::env::var("CLIENT_VERSION").ok();
        if let Some(client_version) = client_version {
            info!("Psyche Client version: {}", client_version);
            if client_version != coordinator_client_version && coordinator_client_version != "test"
            {
                tracing::error!(
                    client_version = %client_version,
                    coordinator_client_version = %coordinator_client_version,
                    "Version mismatch detected. Client version does not match coordinator version."
                );
                std::process::exit(10);
            }
            info!(
                client_version = %client_version,
                coordinator_client_version = %coordinator_client_version,
                "Version check passed"
            );
        } else {
            warn!(
                "Client version env variable was not set - continuing without validating with Coordinator client version"
            )
        }

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
                    coordinator_instance_pubkey,
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

                    match ticked.tick(pending_clients_ids, timestamp, rand::rng().next_u64()) {
                        Ok(_) => {
                            if ticked.run_state != latest_update.run_state {
                                // to avoid *everyone* sending a tick, we probabilisticly send it
                                // targeting having two clients send it per interval
                                let send_tick = match ticked.epoch_state.clients.len() {
                                    0..=2 => true,
                                    len => { let rand: f32 = rand::rng().random();
                                        rand <= 2.0 / len as f32
                                    }
                                };
                                if send_tick {
                                    backend.send_tick(coordinator_instance_pubkey, coordinator_account);
                                }
                            }
                        }
                        Err(CoordinatorError::Halted) => {
                            // If we're waiting to join and the run is halted (paused),
                            // check if the client version has changed, and if it has exit with error code 10 (version mismatch)
                            if joined_run_this_epoch.is_none() && !ever_joined_run {
                                let current_coordinator_state = backend
                                    .get_coordinator_account(&coordinator_account)
                                    .await?;
                                let current_version = String::from(&current_coordinator_state.state.client_version);
                                tracing::debug!(
                                    initial_version = %coordinator_client_version,
                                    current_version = %current_version,
                                    "Run is halted. Checking for client version changes."
                                );

                                if current_version != coordinator_client_version {
                                    tracing::error!(
                                        initial_version = %coordinator_client_version,
                                        current_version = %current_version,
                                        "Client version changed while waiting for run to unpause. Exiting."
                                    );
                                    client.shutdown();
                                    let _ = client.finished().await;
                                    std::process::exit(10);
                                }
                            }
                        },
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
                                        coordinator_instance_pubkey,
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
