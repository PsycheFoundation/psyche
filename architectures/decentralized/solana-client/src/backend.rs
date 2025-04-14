use crate::retry::RetryError;
use anchor_client::{
    anchor_lang::system_program,
    solana_client::{
        nonblocking::{pubsub_client::PubsubClient, rpc_client::RpcClient},
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
use psyche_watcher::{Backend as WatcherBackend, OpportunisticData};
use solana_account_decoder_client_types::{UiAccount, UiAccountEncoding};
use solana_transaction_status_client_types::UiTransactionEncoding;
use std::{cmp::min, sync::Arc, time::Duration};
use tokio::{
    sync::{broadcast, mpsc},
    time::timeout,
};
use tracing::{error, info, trace, warn};

const FORCE_RECONNECTION_TIME: u64 = 30;

pub struct SolanaBackend {
    program_authorizer: Program<Arc<Keypair>>,
    program_coordinator: Arc<Program<Arc<Keypair>>>,
    cluster: Cluster,
    backup_clusters: Vec<Cluster>,
    backup_clients: Vec<Arc<RpcClient>>,
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
    let mut first_connection = true;
    let mut retries: u64 = 0;
    loop {
        let Ok(sub_client) = PubsubClient::new(&url).await else {
            warn!(
                integration_test_log_marker = %IntegrationTestLogMarker::SolanaSubscription,
                url = url,
                subscription_number = id,
                "Solana subscription error, could not connect to url: {url}",
            );

            // wait a time before we try a reconnection
            let sleep_time = min(600, retries.saturating_mul(5));
            tokio::time::sleep(Duration::from_secs(sleep_time)).await;
            retries += 1;
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
                    error = err.to_string(),
                    "Solana account subscribe error",
                );
                return;
            }
        };

        info!(
            integration_test_log_marker = %IntegrationTestLogMarker::SolanaSubscription,
            url = url,
            subscription_number = id,
            "Correctly subscribe to Solana url: {url}",
        );

        // we will force a reconnection to the Solana websocket every 30 minutes
        let refresh_time: u64 = if first_connection {
            FORCE_RECONNECTION_TIME + (((id - 1) * 10) % FORCE_RECONNECTION_TIME)
        } else {
            FORCE_RECONNECTION_TIME
        };
        first_connection = false;
        let refresh_timer = tokio::time::sleep(Duration::from_secs(refresh_time * 60));
        tokio::pin!(refresh_timer);

        loop {
            tokio::select! {
                _ = &mut refresh_timer => {
                    info!(
                        integration_test_log_marker = %IntegrationTestLogMarker::SolanaSubscription,
                        url = url,
                        subscription_number = id,
                        "Force Solana subscription reconnection");
                    break
                }
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
        let sleep_time = min(600, retries.saturating_mul(5));
        tokio::time::sleep(Duration::from_secs(sleep_time)).await;
        retries += 1;
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
        let program_coordinator = Arc::new(client.program(psyche_solana_coordinator::ID)?);
        Ok(Self {
            program_authorizer,
            program_coordinator,
            cluster,
            backup_clients: backup_clusters
                .iter()
                .map(|x| Arc::new(RpcClient::new(x.url().to_string())))
                .collect(),
            backup_clusters,
        })
    }

    pub async fn start(
        self,
        run_id: String,
        coordinator_account: Pubkey,
    ) -> Result<SolanaBackendRunner> {
        let (tx_update, rx_update) = broadcast::channel(32);
        let commitment = self.program_coordinator.rpc().commitment();

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
                                        error!("Error sending coordinator update: {err}");
                                        break;
                                    }
                                    last_nonce = account.nonce;
                                }
                            }
                            Err(err) => error!("Error deserializing coordinator account: {err}"),
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
            &self
                .program_coordinator
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
        metadata: psyche_solana_coordinator::RunMetadata,
        join_authority: Option<Pubkey>,
    ) -> Result<CreatedRun> {
        let space = psyche_solana_coordinator::CoordinatorAccount::space_with_discriminator();
        let rent = self
            .program_coordinator
            .rpc()
            .get_minimum_balance_for_rent_exemption(space)
            .await?;

        let payer = self.program_coordinator.payer();
        let main_authority = payer;
        let join_authority = join_authority.unwrap_or(payer);

        let coordinator_instance = psyche_solana_coordinator::find_coordinator_instance(&run_id);

        let coordinator_account_signer = Arc::new(Keypair::new());
        let coordinator_account = coordinator_account_signer.pubkey();

        let create_coordinator_signature = self
            .program_coordinator
            .request()
            .instruction(system_instruction::create_account(
                &self.program_coordinator.payer(),
                &coordinator_account,
                rent,
                space as u64,
                &self.program_coordinator.id(),
            ))
            .instruction(
                self.program_coordinator
                    .request()
                    .accounts(
                        psyche_solana_coordinator::accounts::InitCoordinatorAccounts {
                            payer,
                            coordinator_instance,
                            coordinator_account,
                            system_program: system_program::ID,
                        },
                    )
                    .args(psyche_solana_coordinator::instruction::InitCoordinator {
                        params: psyche_solana_coordinator::logic::InitCoordinatorParams {
                            main_authority,
                            join_authority,
                            run_id,
                            metadata,
                        },
                    })
                    .instructions()
                    .unwrap()[0]
                    .clone(),
            )
            .signer(coordinator_account_signer.clone())
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
        let payer = self.program_coordinator.payer();
        let authorization_from_payer_to_everyone = psyche_solana_authorizer::find_authorization(
            &payer,
            &system_program::ID,
            psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
        );
        let authorization_create = self
            .program_authorizer
            .request()
            .instruction(
                self.program_authorizer
                    .request()
                    .accounts(
                        psyche_solana_authorizer::accounts::AuthorizationCreateAccounts {
                            payer,
                            grantor: payer,
                            authorization: authorization_from_payer_to_everyone,
                            system_program: system_program::ID,
                        },
                    )
                    .args(psyche_solana_authorizer::instruction::AuthorizationCreate {
                        params: psyche_solana_authorizer::logic::AuthorizationCreateParams {
                            grantee: system_program::ID,
                            scope: psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE
                                .to_vec(),
                        },
                    })
                    .instructions()
                    .unwrap()
                    .remove(0),
            )
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
            .instruction(
                self.program_authorizer
                    .request()
                    .accounts(
                        psyche_solana_authorizer::accounts::AuthorizationGrantorUpdateAccounts {
                            grantor: payer,
                            authorization: authorization_from_payer_to_everyone,
                        },
                    )
                    .args(
                        psyche_solana_authorizer::instruction::AuthorizationGrantorUpdate {
                            params:
                                psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams {
                                    active: true,
                                },
                        },
                    )
                    .instructions()
                    .unwrap()
                    .remove(0),
            )
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
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::FreeCoordinatorAccounts {
                    authority: self.program_coordinator.payer(),
                    spill: self.program_coordinator.payer(),
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
        let signature = self
            .program_coordinator
            .request()
            .accounts(psyche_solana_coordinator::accounts::JoinRunAccounts {
                user: self.program_coordinator.payer(),
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
        let pending_tx = self
            .program_coordinator
            .request()
            .accounts(psyche_solana_coordinator::accounts::JoinRunAccounts {
                user: self.program_coordinator.payer(),
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
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        config: Option<CoordinatorConfig>,
        model: Option<Model>,
        progress: Option<CoordinatorProgress>,
    ) -> Result<Signature> {
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
                    authority: self.program_coordinator.payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(psyche_solana_coordinator::instruction::Update {
                config,
                model,
                progress,
            })
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn set_paused(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        paused: bool,
    ) -> Result<Signature> {
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
                    authority: self.program_coordinator.payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(psyche_solana_coordinator::instruction::SetPaused { paused })
            .send()
            .await?;

        Ok(signature)
    }

    pub async fn set_future_epoch_rates(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        epoch_earning_rate: Option<u64>,
        epoch_slashing_rate: Option<u64>,
    ) -> Result<Signature> {
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::OwnerCoordinatorAccounts {
                    authority: self.program_coordinator.payer(),
                    coordinator_instance,
                    coordinator_account,
                },
            )
            .args(
                psyche_solana_coordinator::instruction::SetFutureEpochRates {
                    epoch_earning_rate,
                    epoch_slashing_rate,
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
        let signature = self
            .program_coordinator
            .request()
            .accounts(
                psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                    user: self.program_coordinator.payer(),
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
        let program_coordinator = self.program_coordinator.clone();
        let backup_clients = self.backup_clients.clone();
        tokio::task::spawn(async move {
            let payer = program_coordinator.payer();
            let pending_tx_builder = program_coordinator
                .request()
                .accounts(
                    psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                        user: payer,
                        coordinator_instance,
                        coordinator_account,
                    },
                )
                .args(psyche_solana_coordinator::instruction::Tick {});
            match pending_tx_builder.signed_transaction().await {
                Ok(signed_tx) => {
                    let pending_tx = pending_tx_builder.send();
                    for client in backup_clients {
                        let signed_tx = signed_tx.clone();
                        tokio::spawn(async move { client.send_transaction(&signed_tx).await });
                    }
                    match pending_tx.await {
                        Ok(signature) => info!(from = %payer, tx = %signature, "Tick transaction"),
                        Err(err) => warn!(from = %payer, "Error sending tick transaction: {err}"),
                    }
                }
                Err(err) => {
                    warn!(from = %payer, "Error signing tick: {err}");
                }
            }
        });
    }

    pub fn send_witness(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        opportunistic_data: OpportunisticData,
    ) {
        let program_coordinator = self.program_coordinator.clone();
        let backup_clients = self.backup_clients.clone();
        tokio::task::spawn(async move {
            let payer = program_coordinator.payer();
            let pending_tx_builder = match opportunistic_data {
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
            };
            match pending_tx_builder.signed_transaction().await {
                Ok(signed_tx) => {
                    let pending_tx = pending_tx_builder.send();
                    for client in backup_clients {
                        let signed_tx = signed_tx.clone();
                        tokio::spawn(async move { client.send_transaction(&signed_tx).await });
                    }
                    match pending_tx.await {
                        Ok(signature) => {
                            info!(from = %payer, tx = %signature, "Witness transaction")
                        }
                        Err(err) => {
                            warn!(from = %payer, "Error sending witness transaction: {err}")
                        }
                    }
                }
                Err(err) => {
                    warn!(from = %payer, "Error signing witness: {err}");
                }
            }
        });
    }

    pub fn send_health_check(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        id: psyche_solana_coordinator::ClientId,
        check: CommitteeProof,
    ) {
        let program_coordinator = self.program_coordinator.clone();
        let backup_clients = self.backup_clients.clone();
        tokio::task::spawn(async move {
            let payer = program_coordinator.payer();
            let pending_tx_builder = program_coordinator
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
                });
            match pending_tx_builder.signed_transaction().await {
                Ok(signed_tx) => {
                    let pending_tx = pending_tx_builder.send();
                    for client in backup_clients {
                        let signed_tx = signed_tx.clone();
                        tokio::spawn(async move { client.send_transaction(&signed_tx).await });
                    }
                    match pending_tx.await {
                        Ok(signature) => {
                            info!(from = %payer, tx = %signature, "Health check transaction")
                        }
                        Err(err) => {
                            warn!(from = %payer, "Error sending health check transaction: {err}")
                        }
                    }
                }
                Err(err) => {
                    warn!(from = %payer, "Error signing health check: {err}");
                }
            }
        });
    }

    pub fn send_checkpoint(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        repo: HubRepo,
    ) {
        let program_coordinator = self.program_coordinator.clone();
        let backup_clients = self.backup_clients.clone();
        tokio::task::spawn(async move {
            let payer = program_coordinator.payer();
            let pending_tx_builder = program_coordinator
                .request()
                .accounts(
                    psyche_solana_coordinator::accounts::PermissionlessCoordinatorAccounts {
                        user: payer,
                        coordinator_instance,
                        coordinator_account,
                    },
                )
                .args(psyche_solana_coordinator::instruction::Checkpoint { repo });
            match pending_tx_builder.signed_transaction().await {
                Ok(signed_tx) => {
                    let pending_tx = pending_tx_builder.send();
                    for client in backup_clients {
                        let signed_tx = signed_tx.clone();
                        tokio::spawn(async move { client.send_transaction(&signed_tx).await });
                    }
                    match pending_tx.await {
                        Ok(signature) => {
                            info!(from = %payer, tx = %signature, "Checkpoint transaction")
                        }
                        Err(err) => {
                            warn!(from = %payer, "Error sending checkpoint transaction: {err}")
                        }
                    }
                }
                Err(err) => {
                    warn!(from = %payer, "Error signing checkpoint: {err}");
                }
            }
        });
    }

    pub async fn get_coordinator_instance(
        &self,
        coordinator_instance: &Pubkey,
    ) -> Result<psyche_solana_coordinator::CoordinatorInstance> {
        let coordinator_instance_state = self
            .program_coordinator
            .account::<psyche_solana_coordinator::CoordinatorInstance>(*coordinator_instance)
            .await
            .context(format!(
                "Unable to get the coordinator_instance: {:?}",
                coordinator_instance
            ))?;
        Ok(coordinator_instance_state)
    }

    pub async fn get_coordinator_account(
        &self,
        coordinator_account: &Pubkey,
    ) -> Result<psyche_solana_coordinator::CoordinatorAccount> {
        let data = self
            .program_coordinator
            .rpc()
            .get_account_data(coordinator_account)
            .await?;
        psyche_solana_coordinator::coordinator_account_from_bytes(&data)
            .map_err(|_| anyhow!("Unable to decode coordinator account data"))
            .copied()
    }

    pub async fn get_balance(&self, account: &Pubkey) -> Result<u64> {
        Ok(self.program_coordinator.rpc().get_balance(account).await?)
    }

    pub async fn get_logs(&self, tx: &Signature) -> Result<Vec<String>> {
        let tx = self
            .program_coordinator
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
