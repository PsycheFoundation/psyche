use crate::instructions::{self, coordinator_tick};
use crate::retry::{RetryError, retry_function_with_params};
use anchor_client::anchor_lang::AccountDeserialize;
use anchor_client::solana_sdk::hash::hash;
use anchor_client::solana_sdk::instruction::Instruction;
use anchor_client::solana_sdk::program_pack::Pack;
use anchor_client::{
    Client, ClientError, Cluster, Program,
    anchor_lang::system_program,
    solana_client::{
        nonblocking::pubsub_client::PubsubClient,
        rpc_config::{RpcAccountInfoConfig, RpcTransactionConfig},
        rpc_response::Response as RpcResponse,
    },
    solana_sdk::{
        commitment_config::CommitmentConfig,
        pubkey::Pubkey,
        signature::{Keypair, Signature, Signer},
    },
};
use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use psyche_coordinator::model::{self, Checkpoint};
use psyche_coordinator::{CommitteeProof, Coordinator, HealthChecks};
use psyche_core::IntegrationTestLogMarker;
use psyche_watcher::{Backend as WatcherBackend, OpportunisticData};
use solana_account_decoder_client_types::{UiAccount, UiAccountEncoding};
use solana_transaction_status_client_types::UiTransactionEncoding;
use std::future::Future;
use std::{cmp::min, sync::Arc, time::Duration};
use tokio::{
    sync::{broadcast, mpsc},
    time::timeout,
};
use tracing::{error, info, trace, warn};

type ProgramCoordinator = Arc<Program<Arc<Keypair>>>;
const RPC_RETRY_ATTEMPTS: usize = 3;
const RPC_INITIAL_BACKOFF_MS: u64 = 500;
const RPC_MAX_BACKOFF_MS: u64 = 10_000;

#[derive(Clone)]
pub struct SolanaBackend {
    program_coordinators: Vec<ProgramCoordinator>,
    cluster: Cluster,
    backup_clusters: Vec<Cluster>,
    wallet: Arc<Keypair>,
}

pub struct SolanaBackendRunner {
    pub(crate) backend: SolanaBackend,
    instance: Pubkey,
    account: Pubkey,
    updates: broadcast::Receiver<Coordinator<psyche_solana_coordinator::ClientId>>,
    init: Option<Coordinator<psyche_solana_coordinator::ClientId>>,
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
        let sleep_time = min(30, retries.saturating_mul(2));
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
        commitment: CommitmentConfig,
    ) -> Result<Self> {
        let client = Client::new_with_options(cluster.clone(), payer.clone(), commitment);

        let mut program_coordinators = vec![];
        program_coordinators.push(Arc::new(client.program(psyche_solana_coordinator::ID)?));

        let backup_program_coordinators: Result<Vec<_>, _> = backup_clusters
            .iter()
            .filter(|backup| backup.url() != cluster.url())
            .map(|cluster| {
                Client::new_with_options(cluster.clone(), payer.clone(), commitment)
                    .program(psyche_solana_coordinator::ID)
            })
            .collect();
        program_coordinators.extend(backup_program_coordinators?.into_iter().map(Arc::new));

        Ok(Self {
            program_coordinators,
            cluster,
            backup_clusters,
            wallet: payer,
        })
    }

    pub async fn start(
        self,
        run_id: String,
        coordinator_account: Pubkey,
    ) -> Result<SolanaBackendRunner> {
        let (tx_update, rx_update) = broadcast::channel(32);
        let commitment_config = self.get_commitment_config();

        let (tx_subscribe, mut rx_subscribe) = mpsc::unbounded_channel();

        let tx_subscribe_ = tx_subscribe.clone();

        let mut subscription_number = 1;
        let url = self.cluster.clone().ws_url().to_string();
        tokio::spawn(async move {
            subscribe_to_account(
                url,
                commitment_config,
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
                    commitment_config,
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

        let init = self
            .get_coordinator_account(&coordinator_account)
            .await?
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

    pub async fn join_run(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        id: psyche_solana_coordinator::ClientId,
        authorizer: Option<Pubkey>,
    ) -> Result<Signature> {
        let coordinator_instance_state =
            self.get_coordinator_instance(&coordinator_instance).await?;
        let authorization =
            Self::find_join_authorization(&coordinator_instance_state.join_authority, authorizer);
        let instruction = instructions::coordinator_join_run(
            &coordinator_instance,
            &coordinator_account,
            &authorization,
            id,
        );
        // TODO (vbrunet) - what was the point of doing specifically a timeout here but not the other TXs ?
        // We timeout the transaction at 5s max, since internally send() polls Solana until the
        // tx is confirmed; we'd rather cancel early and attempt again.
        match timeout(
            Duration::from_secs(5),
            self.send_and_retry("Join run", &[instruction], &[]),
        )
        .await
        {
            Ok(Ok(signature)) => Ok(signature),
            Err(elapsed) => Err(anyhow!("join_run timeout: {elapsed}")),
            Ok(Err(error)) => Err(error),
        }
    }

    pub fn send_tick(&self, coordinator_instance: Pubkey, coordinator_account: Pubkey) {
        let user = self.get_payer();
        let instruction = coordinator_tick(&coordinator_instance, &coordinator_account, &user);
        self.spawn_scheduled_send("Tick", &[instruction], &[]);
    }

    pub fn send_witness(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        opportunistic_data: OpportunisticData,
    ) {
        let user = self.get_payer();
        let instruction = match opportunistic_data {
            OpportunisticData::WitnessStep(witness, metadata) => instructions::coordinator_witness(
                &coordinator_instance,
                &coordinator_account,
                &user,
                witness,
                metadata,
            ),
            OpportunisticData::WarmupStep(witness) => instructions::coordinator_warmup_witness(
                &coordinator_instance,
                &coordinator_account,
                &user,
                witness,
            ),
        };
        self.spawn_scheduled_send("Witness", &[instruction], &[]);
    }

    pub fn send_health_check(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        id: psyche_solana_coordinator::ClientId,
        check: CommitteeProof,
    ) {
        let user = self.get_payer();
        let instruction = instructions::coordinator_health_check(
            &coordinator_instance,
            &coordinator_account,
            &user,
            id,
            check,
        );
        self.spawn_scheduled_send("Health check", &[instruction], &[]);
    }

    pub fn send_checkpoint(
        &self,
        coordinator_instance: Pubkey,
        coordinator_account: Pubkey,
        repo: Checkpoint,
    ) {
        let user = self.get_payer();
        let instruction = instructions::coordinator_checkpoint(
            &coordinator_instance,
            &coordinator_account,
            &user,
            repo,
        );
        self.spawn_scheduled_send("Checkpoint", &[instruction], &[]);
    }

    pub fn find_join_authorization(join_authority: &Pubkey, authorizer: Option<Pubkey>) -> Pubkey {
        psyche_solana_authorizer::find_authorization(
            join_authority,
            &authorizer.unwrap_or(system_program::ID),
            psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE,
        )
    }

    pub async fn get_authorization(
        &self,
        authorization: &Pubkey,
    ) -> Result<psyche_solana_authorizer::state::Authorization> {
        let data = self.get_data(authorization).await?;
        psyche_solana_authorizer::state::Authorization::try_deserialize(&mut data.as_slice())
            .map_err(|error| anyhow!("Unable to decode authorization data: {error}"))
    }

    pub async fn get_coordinator_instance(
        &self,
        coordinator_instance: &Pubkey,
    ) -> Result<psyche_solana_coordinator::CoordinatorInstance> {
        let data = self.get_data(coordinator_instance).await?;
        psyche_solana_coordinator::CoordinatorInstance::try_deserialize(&mut data.as_slice())
            .map_err(|error| anyhow!("Unable to decode coordinator instance data: {error}"))
    }

    pub async fn get_coordinator_account(
        &self,
        coordinator_account: &Pubkey,
    ) -> Result<psyche_solana_coordinator::CoordinatorAccount> {
        let data = self.get_data(coordinator_account).await?;
        psyche_solana_coordinator::coordinator_account_from_bytes(&data)
            .map_err(|error| anyhow!("Unable to decode coordinator account data: {error}"))
            .copied()
    }

    pub async fn get_treasurer_run(
        &self,
        treasurer_run: &Pubkey,
    ) -> Result<psyche_solana_treasurer::state::Run> {
        let data = self.get_data(treasurer_run).await?;
        psyche_solana_treasurer::state::Run::try_deserialize(&mut data.as_slice())
            .map_err(|error| anyhow!("Unable to decode treasurer run data: {error}"))
    }

    pub async fn get_treasurer_participant(
        &self,
        treasurer_participant: &Pubkey,
    ) -> Result<psyche_solana_treasurer::state::Participant> {
        let data = self.get_data(treasurer_participant).await?;
        psyche_solana_treasurer::state::Participant::try_deserialize(&mut data.as_slice())
            .map_err(|error| anyhow!("Unable to decode treasurer participant data: {error}"))
    }

    pub async fn get_token_account(
        &self,
        token_account: &Pubkey,
    ) -> Result<anchor_spl::token::spl_token::state::Account> {
        let data = self.get_data(token_account).await?;
        anchor_spl::token::spl_token::state::Account::unpack(&data)
            .map_err(|error| anyhow!("Unable to decode token account data: {error}"))
    }

    pub async fn get_token_mint(
        &self,
        mint: &Pubkey,
    ) -> Result<anchor_spl::token::spl_token::state::Mint> {
        let data = self.get_data(mint).await?;
        anchor_spl::token::spl_token::state::Mint::unpack(&data)
            .map_err(|error| anyhow!("Unable to decode mint account data: {error}"))
    }

    pub fn compute_deterministic_treasurer_index(
        run_id: &str,
        treasurer_index: Option<u64>,
    ) -> u64 {
        treasurer_index.unwrap_or_else(|| {
            let hashed = hash(run_id.as_bytes()).to_bytes();
            u64::from_le_bytes(hashed[0..8].try_into().unwrap())
        })
    }

    pub async fn resolve_treasurer_index(
        &self,
        run_id: &str,
        treasurer_index: Option<u64>,
    ) -> Result<Option<u64>> {
        let treasurer_index = Self::compute_deterministic_treasurer_index(run_id, treasurer_index);
        let run = psyche_solana_treasurer::find_run(treasurer_index);
        let run_balance = self.get_balance(&run).await?;
        Ok(if run_balance > 0 {
            Some(treasurer_index)
        } else {
            None
        })
    }

    pub fn get_payer(&self) -> Pubkey {
        self.wallet.pubkey()
    }

    pub fn sign_message(&self, message: &[u8]) -> Vec<u8> {
        use anchor_client::solana_sdk::signature::Signer;
        self.wallet.sign_message(message).as_ref().to_vec()
    }

    pub fn get_commitment_config(&self) -> CommitmentConfig {
        self.program_coordinators[0].rpc().commitment()
    }

    pub async fn get_minimum_balance_for_rent_exemption(&self, space: usize) -> Result<u64> {
        self.rpc_with_fallback(
            "get_minimum_balance_for_rent_exemption",
            |coord| async move {
                coord
                    .rpc()
                    .get_minimum_balance_for_rent_exemption(space)
                    .await
                    .map_err(Into::into)
            },
        )
        .await
    }

    pub async fn get_balance(&self, address: &Pubkey) -> Result<u64> {
        let address = *address;
        self.rpc_with_fallback("get_balance", |coord| async move {
            coord.rpc().get_balance(&address).await.map_err(Into::into)
        })
        .await
    }

    pub async fn get_data(&self, address: &Pubkey) -> Result<Vec<u8>> {
        let address = *address;
        self.rpc_with_fallback("get_data", |coord| async move {
            coord
                .rpc()
                .get_account_data(&address)
                .await
                .map_err(Into::into)
        })
        .await
    }

    pub async fn get_logs(&self, tx: &Signature) -> Result<Vec<String>> {
        let tx_sig = *tx;
        let response = self
            .rpc_with_fallback("get_logs", |coord| async move {
                coord
                    .rpc()
                    .get_transaction_with_config(
                        &tx_sig,
                        RpcTransactionConfig {
                            encoding: Some(UiTransactionEncoding::Json),
                            commitment: Some(CommitmentConfig::confirmed()),
                            max_supported_transaction_version: None,
                        },
                    )
                    .await
                    .map_err(Into::into)
            })
            .await?;
        Ok(response
            .transaction
            .meta
            .context("Transaction has no meta information")?
            .log_messages
            .unwrap_or(Vec::new()))
    }

    pub async fn send_and_retry(
        &self,
        name: &str,
        instructions: &[Instruction],
        signers: &[Arc<Keypair>],
    ) -> Result<Signature> {
        info!("Sending transaction: {name}");
        let instructions: Arc<[Instruction]> = instructions.to_vec().into();
        let signers: Arc<[Arc<Keypair>]> = signers.to_vec().into();
        let signature = self
            .rpc_with_fallback(name, |coord| {
                let instructions = instructions.clone();
                let signers = signers.clone();
                async move {
                    let mut request = coord.request();
                    for instruction in instructions.iter() {
                        request = request.instruction(instruction.clone());
                    }
                    for signer in signers.iter() {
                        request = request.signer(signer.clone());
                    }
                    request.send().await.map_err(Into::into)
                }
            })
            .await?;
        info!("Transaction success: {name}, {signature}");
        Ok(signature)
    }

    pub fn spawn_scheduled_send(
        &self,
        name: &str,
        instructions: &[Instruction],
        signers: &[Arc<Keypair>],
    ) {
        let backend = self.clone();
        let name = name.to_string();
        let instructions = instructions.to_vec();
        let signers = signers.to_vec();
        tokio::task::spawn(async move {
            if let Err(err) = backend.send_and_retry(&name, &instructions, &signers).await {
                error!(
                    "Failed to send {} transaction after all retries: {}",
                    name, err
                );
            }
        });
    }

    /// Tries the operation against each RPC in order, with max retries per RPC.
    /// Only falls back to the next RPC on retryable errors (network/timeout).
    async fn rpc_with_fallback<T, F, Fut>(&self, name: &str, f: F) -> Result<T>
    where
        F: Fn(ProgramCoordinator) -> Fut,
        Fut: Future<Output = Result<T, RetryError<ClientError>>>,
    {
        let mut last_error = None;
        for (i, coordinator) in self.program_coordinators.iter().enumerate() {
            let coordinator = coordinator.clone();
            let result = retry_function_with_params(
                name,
                || {
                    let coord = coordinator.clone();
                    f(coord)
                },
                RPC_INITIAL_BACKOFF_MS,
                1.5,
                RPC_RETRY_ATTEMPTS,
                RPC_MAX_BACKOFF_MS,
            )
            .await;

            match result {
                Ok(value) => return Ok(value),
                Err(RetryError::Retryable(e)) => {
                    warn!(
                        integration_test_log_marker = %IntegrationTestLogMarker::RpcFallback,
                        failed_rpc_index = i,
                        next_rpc_index = i + 1,
                        error = %e,
                        "RPC {i} exhausted retries, trying next: {e}",
                    );
                    last_error = Some(e);
                }
                Err(RetryError::NonRetryable(e)) => {
                    return Err(anyhow!("non-retryable RPC error: {e}"));
                }
                Err(RetryError::Fatal(e)) => {
                    return Err(anyhow!("fatal RPC error: {e}"));
                }
            }
        }

        Err(anyhow!(
            "all RPCs exhausted: {}",
            last_error.map(|e| e.to_string()).unwrap_or_default()
        ))
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

    async fn send_checkpoint(&mut self, checkpoint: model::Checkpoint) -> Result<()> {
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
