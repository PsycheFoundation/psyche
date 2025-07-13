use crate::instructions;
use crate::retry::RetryError;
use anchor_client::{
    anchor_lang::system_program,
    solana_client::{
        nonblocking::pubsub_client::PubsubClient,
        rpc_config::{RpcAccountInfoConfig, RpcSendTransactionConfig, RpcTransactionConfig},
        rpc_response::Response as RpcResponse,
    },
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{Keypair, Signature, Signer},
        system_instruction,
    },
    Client, Cluster, Program,
};
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use psyche_client::IntegrationTestLogMarker;
use psyche_coordinator::{
    model::{HubRepo, Model},
    CommitteeProof, Coordinator, CoordinatorConfig, CoordinatorProgress, HealthChecks,
};
use psyche_solana_coordinator::RunMetadata;
use psyche_watcher::{Backend as WatcherBackend, OpportunisticData};
use solana_account_decoder_client_types::{UiAccount, UiAccountEncoding};
use solana_transaction_status_client_types::UiTransactionEncoding;
use std::{cmp::min, sync::Arc, time::Duration};
use tokio::{
    sync::{broadcast, mpsc},
    time::timeout,
};
use tracing::{debug, error, info, trace, warn};

const SEND_RETRIES: usize = 3;

pub struct SolanaBackend {
    program_authorizer: Program<Arc<Keypair>>,
    program_coordinators: Vec<Arc<Program<Arc<Keypair>>>>,
    cluster: Cluster,
    backup_clusters: Vec<Cluster>,
}

pub struct SolanaBackendRunner {
    pub(crate) backend: SolanaBackend,
    instance: Pubkey,
    account: Pubkey,
    updates: broadcast::Receiver<Coordinator<psyche_solana_coordinator::ClientId>>,
    init: Option<Coordinator<psyche_solana_coordinator::ClientId>>,
}

#[derive(Debug, Clone)]
pub struct CreatedRun {
    pub instance: Pubkey,
    pub account: Pubkey,
    pub create_signatures: Vec<Signature>,
}

async fn subscribe_to_account(
    url: String,
    commitment: CommitmentConfig,
    coordinator_account: &Pubkey,
    tx: mpsc::UnboundedSender<RpcResponse<UiAccount>>,
    id: u64,
) {
    let mut retries: u64 = 0;
    loop {
        // wait a time before we try a reconnection
        let sleep_time = min(600, retries.saturating_mul(5));
        tokio::time::sleep(Duration::from_secs(sleep_time)).await;
        retries += 1;
        let Ok(sub_client) = PubsubClient::new(&url).await else {
            warn!(
                integration_test_log_marker = %IntegrationTestLogMarker::SolanaSubscription,
                url = url,
                subscription_number = id,
                "Solana subscription error, could not connect to url: {url}",
            );
            continue;
        };

        let mut notifications = match sub_client
            .account_subscribe(
                coordinator_account,
                Some(RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64Zstd),
                    commitment: Some(commitment),
                    ..Default::default()
                }),
            )
            .await
        {
            Ok((notifications, _)) => notifications,
            Err(err) => {
                error!(
                    url = url,
                    subscription_number = id,
                    error = format!("{err:#}"),
                    "Solana account subscribe error",
                );
                continue;
            }
        };

        info!(
            integration_test_log_marker = %IntegrationTestLogMarker::SolanaSubscription,
            url = url,
            subscription_number = id,
            "Correctly subscribe to Solana url: {url}",
        );

        retries = 0;

        loop {
            tokio::select! {
                update = notifications.next() => {
                    match update {
                        Some(data) => {
                                if tx.send(data).is_err() {
                                    break;
                                }
                        }
                        None => {
                            warn!(
                                integration_test_log_marker = %IntegrationTestLogMarker::SolanaSubscription,
                                url = url,
                                subscription_number = id,
                                "Solana subscription error, websocket closed");
                            break
                        }
                    }
                }
            }
        }
    }
}

impl SolanaBackend {
    #[allow(dead_code)]
    pub fn new(
        cluster: Cluster,
        backup_clusters: Vec<Cluster>,
        payer: Arc<Keypair>,
        committment: CommitmentConfig,
    ) -> Result<Self> {
        let client = Client::new_with_options(cluster.clone(), payer.clone(), committment);
        let program_authorizer = client.program(psyche_solana_authorizer::ID)?;

        let mut program_coordinators = vec![];
        program_coordinators.push(Arc::new(client.program(psyche_solana_coordinator::ID)?));

        let backup_program_coordinators: Result<Vec<_>, _> = backup_clusters
            .iter()
            .map(|cluster| {
                Client::new_with_options(cluster.clone(), payer.clone(), committment)
                    .program(psyche_solana_coordinator::ID)
            })
            .collect();
        program_coordinators.extend(backup_program_coordinators?.into_iter().map(Arc::new));

        Ok(Self {
            program_authorizer,
            program_coordinators,
            cluster,
            backup_clusters,
        })
    }

    pub async fn start(
        self,
        run_id: String,
        coordinator_account: Pubkey,
    ) -> Result<SolanaBackendRunner> {
        let (tx_update, rx_update) = broadcast::channel(32);
        let commitment = self.program_coordinators[0].rpc().commitment();

        let (tx_subscribe, mut rx_subscribe) = mpsc::unbounded_channel();

        let tx_subscribe_ = tx_subscribe.clone();

        let mut subscription_number = 1;
        let url = self.cluster.clone().ws_url().to_string();
        tokio::spawn(async move {
            subscribe_to_account(
                url,
                commitment,
                &coordinator_account,
                tx_subscribe_,
                subscription_number,
            )
            .await
        });

        for cluster in self.backup_clusters.clone() {
            subscription_number += 1;
            let tx_subscribe_ = tx_subscribe.clone();
            tokio::spawn(async move {
                subscribe_to_account(
                    cluster.ws_url().to_string().clone(),
                    commitment,
                    &coordinator_account,
                    tx_subscribe_,
                    subscription_number,
                )
                .await
            });
        }
        tokio::spawn(async move {
            let mut last_nonce = 0;
            while let Some(update) = rx_subscribe.recv().await {
                match update.value.data.decode() {
                    Some(data) => {
                        match psyche_solana_coordinator::coordinator_account_from_bytes(&data) {
                            Ok(account) => {
                                if account.nonce > last_nonce {
                                    trace!(
                                        nonce = account.nonce,
                                        last_nonce = last_nonce,
                                        "Coordinator account update"
                                    );
                                    if let Err(err) = tx_update.send(account.state.coordinator) {
                                        error!("Error sending coordinator update: {err:#}");
                                        break;
                                    }
                                    last_nonce = account.nonce;
                                }
                            }
                            Err(err) => error!("Error deserializing coordinator account: {err:#}"),
                        }
                    }
                    None => error!("Error decoding coordinator account"),
                }
            }
            error!("No subscriptions available");
        });

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);

        info!("Coordinator account address: {}", coordinator_account);
        info!(
            "Coordinator instance address for run \"{}\": {}",
            run_id, coordinator_instance
        );

        let init = psyche_solana_coordinator::coordinator_account_from_bytes(
            &self.program_coordinators[0]
                .rpc()
                .get_account_data(&coordinator_account)
                .await?,
        )?
        .state
        .coordinator;

        Ok(SolanaBackendRunner {
            backend: self,
            updates: rx_update,
            instance: coordinator_instance,
            account: coordinator_account,
            init: Some(init),
        })
    }

    pub async fn create_run(
        &self,
        run_id: String,
        join_authority: Option<Pubkey>,
        treasurer_collateral_mint: Option<Pubkey>,
    ) -> Result<CreatedRun> {
        let space = psyche_solana_coordinator::CoordinatorAccount::space_with_discriminator();
        let rent = self.program_coordinators[0]
            .rpc()
            .get_minimum_balance_for_rent_exemption(space)
            .await?;

        let payer = self.program_coordinators[0].payer();
        let main_authority = payer;
        let join_authority = join_authority.unwrap_or(payer);

        let coordinator_account_signer = Keypair::new();
        let coordinator_account = coordinator_account_signer.pubkey();

        let create_coordinator_signature = self.program_coordinators[0]
            .request()
            .instruction(system_instruction::create_account(
                &self.program_coordinators[0].payer(),
                &coordinator_account,
                rent,
                space as u64,
                &self.program_coordinators[0].id(),
            ))
            .instruction(
                if let Some(treasurer_collateral_mint) = treasurer_collateral_mint {
                    instructions::treasurer_run_create(
                        &payer,
                        &run_id,
                        &treasurer_collateral_mint,
                        &coordinator_account,
                        &main_authority,
                        &join_authority,
                    )
                } else {
                    instructions::coordinator_init(
                        &payer,
                        &run_id,
                        &coordinator_account,
                        &main_authority,
                        &join_authority,
                    )
                },
            )
            .signer(coordinator_account_signer)
            .send()
            .await?;

        let mut create_signatures = vec![create_coordinator_signature];

        if join_authority == payer {
            let (authorization_create, authorization_activate) =
                self.create_run_ensure_permissionless().await?;
            create_signatures.push(authorization_create);
            create_signatures.push(authorization_activate);
        }

        Ok(CreatedRun {
            instance: coordinator_instance,
            account: coordinator_account,
            create_signatures,
        })
    }

    async fn create_run_ensure_permissionless(&self) -> Result<(Signature, Signature)> {
        let payer = self.program_coordinators[0].payer();
        let authorization_create = self
            .program_authorizer
            .request()
            .instruction(instructions::authorizer_authorization_create(
                &payer,
                &payer,
                &system_program::ID,
                psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
            ))
            .send_with_spinner_and_config(RpcSendTransactionConfig {
                skip_preflight: true,
                preflight_commitment: None,
                encoding: None,
                max_retries: None,
                min_context_slot: None,
            })
            .await?;
        let authorization_activate = self
            .program_authorizer
            .request()
            .instruction(instructions::authorizer_authorization_grantor_update(
                &payer,
                &system_program::ID,
                psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
                true,
            ))
            .send_with_spinner_and_config(RpcSendTransactionConfig {
                skip_preflight: true,
                preflight_commitment: None,
                encoding: None,
                max_retries: None,
                min_context_slot: None,
            })
            .await?;
        Ok((authorization_create, authorization_activate))
    }

    pub async fn close_run(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
    ) -> Result<Signature> {
        let signature = self.program_coordinators[0]
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::FreeCoordinatorAccounts {
                    authority: self.program_coordinators[0].payer(),
                    spill: self.program_coordinators[0].payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(psyche_solana_coordinator::instruction::FreeCoordinator {
                params: psyche_solana_coordinator::logic::FreeCoordinatorParams {},
            })
            .send()
            .await?;

        Ok(signature)
    }

    #[allow(unused)]
    pub async fn join_run(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        id: psyche_solana_coordinator::ClientId,
        authorizer: Option<Pubkey>,
    ) -> Result<Signature> {
        let coordinator_instance_state =
            self.get_coordinator_instance(&coordinator_instance).await?;
        let authorization = psyche_solana_authorizer::find_authorization(
            &coordinator_instance_state.join_authority,
            &authorizer.unwrap_or(system_program::ID),
            psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
        );
        let signature = self.program_coordinators[0]
            .request()
            .accounts(psyche_solana_coordinator::accounts::JoinRunAccounts {
                user: self.program_coordinators[0].payer(),
                authorization,
                coordinator_instance,
                coordinator_account,
            })
            .args(psyche_solana_coordinator::instruction::JoinRun {
                params: psyche_solana_coordinator::logic::JoinRunParams { client_id: id },
            })
            .send()
            .await?;
        Ok(signature)
    }

    pub async fn join_run_retryable(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        id: psyche_solana_coordinator::ClientId,
        authorizer: Option<Pubkey>,
    ) -> Result<Signature, RetryError<String>> {
        let coordinator_instance_state = self
            .get_coordinator_instance(&coordinator_instance)
            .await
            .map_err(|err| RetryError::Fatal(err.to_string()))?;
        let authorization_global = psyche_solana_authorizer::find_authorization(
            &coordinator_instance_state.join_authority,
            &authorizer.unwrap_or(system_program::ID),
            psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
        );
        let pending_tx = self.program_coordinators[0]
            .request()
            .accounts(psyche_solana_coordinator::accounts::JoinRunAccounts {
                user: self.program_coordinators[0].payer(),
                authorization: authorization_global,
                coordinator_instance,
                coordinator_account,
            })
            .args(psyche_solana_coordinator::instruction::JoinRun {
                params: psyche_solana_coordinator::logic::JoinRunParams { client_id: id },
            })
            .send();

        // We timeout the transaction at 5s max, since internally send() polls Solana until the
        // tx is confirmed; we'd rather cancel early and attempt again.
        match timeout(Duration::from_secs(5), pending_tx).await {
            Ok(Ok(s)) => Ok(s),
            Err(_elapsed) => {
                error!("[TIMEOUT] join_run_retryable");
                Err(RetryError::non_retryable_error(
                    "timeout join_run_retryable",
                ))
            }
            Ok(Err(e)) => {
                warn!("join_run_retryable error: {}", e);
                Err(RetryError::from(e).into())
            }
        }
    }

    pub async fn update(
        &self,
        run_id: &str,
        treasurer_collateral_mint: Option<Pubkey>,
        coordinator_account: &Pubkey,
        metadata: Option<RunMetadata>,
        config: Option<CoordinatorConfig>,
        model: Option<Model>,
        progress: Option<CoordinatorProgress>,
    ) -> Result<Signature> {
        let main_authority = self.program_coordinators[0].payer();
        let signature = self.program_coordinators[0]
            .request()
            .instruction(
                if let Some(treasurer_collateral_mint) = treasurer_collateral_mint {
                    instructions::treasurer_run_update(
                        run_id,
                        treasurer_collateral_mint,
                        coordinator_account,
                        main_authority,
                        RunUpdateParams {
                            metadata,
                            config,
                            model,
                            progress,
                            epoch_earning_rate: None,
                            epoch_slashing_rate: None,
                            paused: None,
                        },
                    )
                } else {
                    instructions::coordinator_update(
                        run_id,
                        coordinator_account,
                        main_authority,
                        metadata,
                        config,
                        model,
                        progress,
                    )
                },
            )
            .send()
            .await?;
        Ok(signature)
    }

    pub async fn set_paused(
        &self,
        run_id: &str,
        treasurer_collateral_mint: Option<Pubkey>,
        coordinator_account: &Pubkey,
        paused: bool,
    ) -> Result<Signature> {
        let main_authority = self.program_coordinators[0].payer();
        let signature = self.program_coordinators[0]
            .request()
            .instruction(
                if let Some(treasurer_collateral_mint) = treasurer_collateral_mint {
                    instructions::treasurer_run_update(
                        run_id,
                        treasurer_collateral_mint,
                        coordinator_account,
                        main_authority,
                        RunUpdateParams {
                            metadata: None,
                            config: None,
                            model: None,
                            progress: None,
                            epoch_earning_rate: None,
                            epoch_slashing_rate: None,
                            paused,
                        },
                    )
                } else {
                    instructions::coordinator_set_paused(
                        run_id,
                        coordinator_account,
                        main_authority,
                        paused,
                    )
                },
            )
            .send()
            .await?;
        Ok(signature)
    }

    pub async fn set_future_epoch_rates(
        &self,
        run_id: &str,
        treasurer_collateral_mint: Option<Pubkey>,
        coordinator_account: &Pubkey,
        epoch_earning_rate: Option<u64>,
        epoch_slashing_rate: Option<u64>,
    ) -> Result<Signature> {
        let main_authority = self.program_coordinators[0].payer();
        let signature = self.program_coordinators[0]
            .request()
            .instruction(
                if let Some(treasurer_collateral_mint) = treasurer_collateral_mint {
                    instructions::treasurer_run_update(
                        run_id,
                        treasurer_collateral_mint,
                        coordinator_account,
                        main_authority,
                        RunUpdateParams {
                            metadata: None,
                            config: None,
                            model: None,
                            progress: None,
                            epoch_earning_rate,
                            epoch_slashing_rate,
                            paused: None,
                        },
                    )
                } else {
                    instructions::coordinator_set_future_epoch_rates(
                        run_id,
                        coordinator_account,
                        main_authority,
                        epoch_earning_rate,
                        epoch_slashing_rate,
                    )
                },
            )
            .send()
            .await?;
        Ok(signature)
    }

    pub async fn tick(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
    ) -> Result<Signature> {
        let signature = self.program_coordinators[0]
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                    user: self.program_coordinators[0].payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(psyche_solana_coordinator::instruction::Tick {})
            .send()
            .await?;

        Ok(signature)
    }

    pub fn send_tick(&self, coordinator_instance: Pubkey, coordinator_account: Pubkey) {
        let program_coordinators = self.program_coordinators.clone();
        tokio::task::spawn(async move {
            for _ in 0..SEND_RETRIES {
                for program_coordinator in &program_coordinators {
                    let payer = program_coordinator.payer();
                    let pending_tx = program_coordinator
                        .request()
                        .accounts(
                            psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                                user: payer,
                                coordinator_instance,
                                coordinator_account,
                            },
                        )
                        .args(psyche_solana_coordinator::instruction::Tick {})
                        .send();
                    match pending_tx.await {
                        Ok(tx) => {
                            info!(from = %payer, tx = %tx, "Tick transaction");
                            return;
                        }
                        Err(err) => debug!(from = %payer, "Error sending tick transaction: {err}"),
                    }
                }
            }
            error!(from = %program_coordinators[0].payer(), "All attempts to send tick transaction failed")
        });
    }

    pub fn send_witness(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        opportunistic_data: OpportunisticData,
    ) {
        let program_coordinators = self.program_coordinators.clone();
        tokio::task::spawn(async move {
            for _ in 0..SEND_RETRIES {
                for program_coordinator in &program_coordinators {
                    let payer = program_coordinator.payer();
                    let pending_tx = match opportunistic_data {
                        OpportunisticData::WitnessStep(witness, metadata) => program_coordinator
                            .request()
                            .accounts(
                                psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                                    user: payer,
                                    coordinator_instance,
                                    coordinator_account,
                                },
                            )
                            .args(psyche_solana_coordinator::instruction::Witness {
                                proof: witness.proof,
                                participant_bloom: witness.participant_bloom,
                                broadcast_bloom: witness.broadcast_bloom,
                                broadcast_merkle: witness.broadcast_merkle,
                                metadata,
                            }),
                        OpportunisticData::WarmupStep(witness) => program_coordinator
                            .request()
                            .accounts(
                                psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                                    user: payer,
                                    coordinator_instance,
                                    coordinator_account,
                                },
                            )
                            .args(psyche_solana_coordinator::instruction::WarmupWitness {
                                proof: witness.proof,
                                participant_bloom: witness.participant_bloom,
                                broadcast_bloom: witness.broadcast_bloom,
                                broadcast_merkle: witness.broadcast_merkle,
                            }),
                    }.send();
                    match pending_tx.await {
                        Ok(tx) => {
                            info!(from = %payer, tx = %tx, "Witness transaction");
                            return;
                        }
                        Err(err) => {
                            warn!(from = %payer, "Error sending witness transaction: {err}")
                        }
                    }
                }
            }
            error!(from = %program_coordinators[0].payer(), "All attempts to send witness transaction failed");
        });
    }

    pub fn send_health_check(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        id: psyche_solana_coordinator::ClientId,
        check: CommitteeProof,
    ) {
        let program_coordinators = self.program_coordinators.clone();
        tokio::task::spawn(async move {
            for _ in 0..SEND_RETRIES {
                for program_coordinator in &program_coordinators {
                    let payer = program_coordinator.payer();
                    let pending_tx = program_coordinator
                        .request()
                        .accounts(
                            psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                                user: payer,
                                coordinator_instance,
                                coordinator_account,
                            },
                        )
                        .args(psyche_solana_coordinator::instruction::HealthCheck {
                            id,
                            committee: check.committee,
                            position: check.position,
                            index: check.index,
                        }).send();
                    match pending_tx.await {
                        Ok(tx) => {
                            info!(from = %payer, tx = %tx, "Health check transaction");
                            return;
                        }
                        Err(err) => {
                            warn!(from = %payer, "Error sending health check transaction: {err}")
                        }
                    }
                }
            }
            error!(from = %program_coordinators[0].payer(), "All attempts to send health check transaction failed");
        });
    }

    pub async fn checkpoint(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        repo: HubRepo,
    ) -> Result<Signature> {
        let payer = self.program_coordinators[0].payer();
        let signature = self.program_coordinators[0]
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                    user: payer,
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(psyche_solana_coordinator::instruction::Checkpoint { repo })
            .send()
            .await?;
        Ok(signature)
    }

    pub fn send_checkpoint(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        repo: HubRepo,
    ) {
        let program_coordinators = self.program_coordinators.clone();
        tokio::task::spawn(async move {
            for _ in 0..SEND_RETRIES {
                for program_coordinator in &program_coordinators {
                    let payer = program_coordinator.payer();
                    let pending_tx = program_coordinator
                        .request()
                        .accounts(
                            psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                                user: payer,
                                coordinator_instance,
                                coordinator_account,
                            },
                        )
                        .args(psyche_solana_coordinator::instruction::Checkpoint { repo })
                        .send();
                    match pending_tx.await {
                        Ok(tx) => {
                            info!(from = %payer, tx = %tx, "Checkpoint transaction");
                            return;
                        }
                        Err(err) => {
                            warn!(from = %payer, "Error sending checkpoint transaction: {err}")
                        }
                    }
                }
            }
            error!(from = %program_coordinators[0].payer(), "All attempts to send checkpoint transaction failed");
        });
    }

    pub async fn get_coordinator_instance(
        &self,
        coordinator_instance: &Pubkey,
    ) -> Result<psyche_solana_coordinator::CoordinatorInstance> {
        let coordinator_instance_state = self.program_coordinators[0]
            .account::<psyche_solana_coordinator::CoordinatorInstance>(*coordinator_instance)
            .await
            .context(format!(
                "Unable to get the coordinator_instance: {coordinator_instance:?}"
            ))?;
        Ok(coordinator_instance_state)
    }

    pub async fn get_coordinator_account(
        &self,
        coordinator_account: &Pubkey,
    ) -> Result<psyche_solana_coordinator::CoordinatorAccount> {
        let data = self.program_coordinators[0]
            .rpc()
            .get_account_data(coordinator_account)
            .await?;
        psyche_solana_coordinator::coordinator_account_from_bytes(&data)
            .map_err(|_| anyhow!("Unable to decode coordinator account data"))
            .copied()
    }

    pub async fn get_balance(&self, account: &Pubkey) -> Result<u64> {
        Ok(self.program_coordinators[0]
            .rpc()
            .get_balance(account)
            .await?)
    }

    pub async fn get_logs(&self, tx: &Signature) -> Result<Vec<String>> {
        let tx = self.program_coordinators[0]
            .rpc()
            .get_transaction_with_config(
                tx,
                RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::Json),
                    commitment: Some(CommitmentConfig::confirmed()),
                    max_supported_transaction_version: None,
                },
            )
            .await?;

        Ok(tx
            .transaction
            .meta
            .ok_or(anyhow!("Transaction has no meta information"))?
            .log_messages
            .unwrap_or(Vec::new()))
    }
}

#[async_trait::async_trait]
impl WatcherBackend<psyche_solana_coordinator::ClientId> for SolanaBackendRunner {
    async fn wait_for_new_state(
        &mut self,
    ) -> Result<Coordinator<psyche_solana_coordinator::ClientId>> {
        match self.init.take() {
            Some(init) => Ok(init),
            None => self
                .updates
                .recv()
                .await
                .map_err(|err| anyhow!("Error receiving new state: {err}")),
        }
    }

    async fn send_witness(&mut self, opportunistic_data: OpportunisticData) -> Result<()> {
        self.backend
            .send_witness(self.instance, self.account, opportunistic_data);
        Ok(())
    }

    async fn send_health_check(
        &mut self,
        checks: HealthChecks<psyche_solana_coordinator::ClientId>,
    ) -> Result<()> {
        for (id, proof) in checks {
            self.backend
                .send_health_check(self.instance, self.account, id, proof);
        }
        Ok(())
    }

    async fn send_checkpoint(&mut self, checkpoint: HubRepo) -> Result<()> {
        self.backend
            .send_checkpoint(self.instance, self.account, checkpoint);
        Ok(())
    }
}

impl SolanaBackendRunner {
    pub fn updates(&self) -> broadcast::Receiver<Coordinator<psyche_solana_coordinator::ClientId>> {
        self.updates.resubscribe()
    }
}
