use anchor_client::solana_sdk::{
    commitment_config::CommitmentConfig, pubkey::Pubkey, system_program,
};
use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result, anyhow};
use psyche_coordinator::RunState;
use psyche_solana_authorizer::state::Authorization;
use psyche_solana_coordinator::{
    CoordinatorInstance, coordinator_account_from_bytes, find_coordinator_instance,
    logic::JOIN_RUN_AUTHORIZATION_SCOPE,
};
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct RunInfo {
    pub run_id: String,
    pub instance_pubkey: Pubkey,
    pub coordinator_account: Pubkey,
    pub run_state: RunState,
}

/// Coordinator client for querying Solana
pub struct CoordinatorClient {
    rpc_client: RpcClient,
    program_id: Pubkey,
}

impl CoordinatorClient {
    pub fn new(rpc_endpoint: String, program_id: Pubkey) -> Self {
        let rpc_client =
            RpcClient::new_with_commitment(rpc_endpoint, CommitmentConfig::confirmed());
        Self {
            rpc_client,
            program_id,
        }
    }

    // Fetch coordinator data and deserialize into a struct
    pub fn fetch_coordinator_data(&self, run_id: &str) -> Result<CoordinatorInstance> {
        // Derive the coordinator instance PDA
        let coordinator_instance = find_coordinator_instance(run_id);

        let account = self
            .rpc_client
            .get_account(&coordinator_instance)
            .context("RPC error: failed to get account")?;

        let instance = CoordinatorInstance::try_deserialize(&mut account.data.as_slice())
            .context("Failed to deserialize CoordinatorInstance")?;

        Ok(instance)
    }

    fn fetch_run_state(&self, coordinator_account: &Pubkey) -> Result<RunState> {
        // Fetch the raw Solana account data from the blockchain
        let solana_account = self
            .rpc_client
            .get_account(coordinator_account)
            .with_context(|| {
                format!(
                    "Failed to fetch coordinator account {}",
                    coordinator_account
                )
            })?;

        // Deserialize the account data into a CoordinatorAccount struct
        let coordinator =
            coordinator_account_from_bytes(&solana_account.data).with_context(|| {
                format!(
                    "Failed to deserialize coordinator account {}",
                    coordinator_account
                )
            })?;

        Ok(coordinator.state.coordinator.run_state)
    }

    pub fn get_docker_tag_for_run(&self, run_id: &str, local_docker: bool) -> Result<String> {
        info!("Querying coordinator for Run ID: {}", run_id);

        let instance = self.fetch_coordinator_data(run_id)?;

        // Fetch the coordinator account to get the client version
        let coordinator_account_data =
            self.rpc_client.get_account(&instance.coordinator_account)?;
        let coordinator_account = coordinator_account_from_bytes(&coordinator_account_data.data)?;

        let client_version = String::from(&coordinator_account.state.client_version);

        info!(
            "Fetched CoordinatorInstance from chain: {{ run_id: {}, coordinator_account: {}, client_version: {} }}",
            instance.run_id, instance.coordinator_account, client_version
        );

        // Depending on how the version is specified in the Coordinator, we should format
        // it accordingly. When specifing a RepoId SHA256, we use
        //      <image_name>@sha256:<repo_id>
        // if not using the RepoId hash, we just want
        //      <image_name>:<version>
        // Also, if using the --local flag (only relevant for testing) the image name is
        // just the local ImageId of the docker image
        let image_name = if client_version.starts_with("sha256:") {
            if local_docker {
                client_version
            } else {
                format!("nousresearch/psyche-client@{}", client_version)
            }
        } else if local_docker {
            format!("psyche-solana-client:{}", client_version)
        } else {
            format!("nousresearch/psyche-client:{}", client_version)
        };

        Ok(image_name)
    }

    pub fn get_all_runs(&self) -> Result<Vec<RunInfo>> {
        // Fetch all CoordinatorInstance accounts that are owned by the program
        let accounts = self
            .rpc_client
            .get_program_accounts_with_config(
                &self.program_id,
                RpcProgramAccountsConfig {
                    account_config: RpcAccountInfoConfig {
                        encoding: Some(UiAccountEncoding::Base64),
                        commitment: Some(CommitmentConfig::confirmed()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to fetch program accounts from coordinator program {}: {}",
                    self.program_id,
                    e
                )
            })?;

        let mut runs = Vec::new();
        for (pubkey, account) in accounts {
            match CoordinatorInstance::try_deserialize(&mut account.data.as_slice()) {
                Ok(instance) => {
                    if let Ok(run_state) = self.fetch_run_state(&instance.coordinator_account) {
                        runs.push(RunInfo {
                            run_id: instance.run_id.clone(),
                            instance_pubkey: pubkey,
                            coordinator_account: instance.coordinator_account,
                            run_state,
                        });
                    } else {
                        debug!(
                            "Skipping run {} (instance: {}) - could not fetch coordinator state",
                            instance.run_id, pubkey
                        );
                    }
                }
                Err(e) => {
                    debug!(
                        "Failed to deserialize CoordinatorInstance at {}: {}",
                        pubkey, e
                    );
                }
            }
        }

        Ok(runs)
    }

    /// Check if a user is authorized to join a specific run.
    ///
    /// This checks both permissionless authorization (grantee = system_program::ID)
    /// and user-specific authorization (grantee = user_pubkey),
    /// as well as delegate-key authorization..
    /// Returns the matched grantee pubkey if authorized, or None if not.
    pub fn can_user_join_run(&self, run_id: &str, user_pubkey: &Pubkey) -> Result<Option<Pubkey>> {
        // Fetch the CoordinatorInstance to get join_authority
        let instance = self.fetch_coordinator_data(run_id)?;
        let join_authority = instance.join_authority;

        // Try permissionless authorization (grantee = system_program::ID)
        if self.check_authorization_for_grantee(&join_authority, &system_program::ID, user_pubkey) {
            return Ok(Some(system_program::ID));
        }

        // Try user-specific authorization (grantee = user_pubkey)
        if self.check_authorization_for_grantee(&join_authority, user_pubkey, user_pubkey) {
            return Ok(Some(*user_pubkey));
        }

        // If we reached here attempt to join as a delegate key via AUTHORIZER env var
        info!("Attempting authorization via delegate key...");
        let Ok(authorizer_str) = std::env::var("AUTHORIZER") else {
            return Err(anyhow!("AUTHORIZER not set"));
        };
        let Ok(authorizer) = authorizer_str.parse::<Pubkey>() else {
            return Err(anyhow!("Failed to parse AUTHORIZER as pubkey"));
        };
        if self.check_authorization_for_grantee(&join_authority, &authorizer, user_pubkey) {
            return Ok(Some(authorizer));
        }

        Ok(None)
    }

    /// Check if an authorization exists and is valid for a specific grantee.
    fn check_authorization_for_grantee(
        &self,
        join_authority: &Pubkey,
        grantee: &Pubkey,
        user_pubkey: &Pubkey,
    ) -> bool {
        let auth_pda = psyche_solana_authorizer::find_authorization(
            join_authority,
            grantee,
            JOIN_RUN_AUTHORIZATION_SCOPE,
        );

        let Ok(account) = self.rpc_client.get_account(&auth_pda) else {
            return false;
        };

        let Ok(authorization) = Authorization::try_deserialize(&mut account.data.as_slice()) else {
            warn!(
                "Failed to deserialize authorization at {}: invalid data",
                auth_pda
            );
            return false;
        };

        authorization.is_valid_for(join_authority, user_pubkey, JOIN_RUN_AUTHORIZATION_SCOPE)
    }
}
