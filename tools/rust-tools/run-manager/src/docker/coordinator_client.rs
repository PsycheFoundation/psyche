use anchor_client::solana_sdk::{
    commitment_config::CommitmentConfig, pubkey::Pubkey, system_program,
};
use anchor_lang::AccountDeserialize;
use anyhow::{Context, Result};
use psyche_coordinator::RunState;
use psyche_solana_authorizer::state::Authorization;
use psyche_solana_coordinator::{
    CoordinatorInstance, coordinator_account_from_bytes, find_coordinator_instance,
    logic::JOIN_RUN_AUTHORIZATION_SCOPE,
};
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use std::time::SystemTime;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct RunInfo {
    pub run_id: String,
    pub instance_pubkey: Pubkey,
    pub coordinator_account: Pubkey,
    pub run_state: RunState,
    pub num_clients: usize,
    pub min_clients: u16,
    pub epoch_time_secs: u64,
    pub epoch_start_timestamp: u64,
}

fn format_duration_secs(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;

    let mut parts = Vec::new();
    if h > 0 {
        parts.push(format!("{}h", h));
    }
    if m > 0 {
        parts.push(format!("{}m", m));
    }
    if s > 0 || parts.is_empty() {
        parts.push(format!("{}s", s));
    }

    parts.join(" ")
}

impl RunInfo {
    pub fn time_remaining_display(&self) -> String {
        match self.run_state {
            RunState::WaitingForMembers
            | RunState::Finished
            | RunState::Paused
            | RunState::Uninitialized => return "-".to_string(),
            _ => {}
        }
        if self.epoch_start_timestamp == 0 {
            return "-".to_string();
        }
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let elapsed = now.saturating_sub(self.epoch_start_timestamp);
        if elapsed >= self.epoch_time_secs {
            "overrun".to_string()
        } else {
            format_duration_secs(self.epoch_time_secs - elapsed)
        }
    }

    pub fn clients_display(&self) -> String {
        match self.run_state {
            RunState::Paused | RunState::Uninitialized | RunState::Finished => "-".to_string(),
            _ => {
                if (self.num_clients as u16) < self.min_clients {
                    format!("{}/{} waiting", self.num_clients, self.min_clients)
                } else {
                    format!("{} training", self.num_clients)
                }
            }
        }
    }

    pub fn format_table(runs: &[&RunInfo]) -> Vec<String> {
        let rows: Vec<_> = runs
            .iter()
            .map(|r| {
                (
                    r.run_id.as_str(),
                    r.run_state.to_string(),
                    r.clients_display(),
                    format_duration_secs(r.epoch_time_secs),
                    r.time_remaining_display(),
                )
            })
            .collect();

        // Header labels
        let headers = ("run-id", "state", "clients", "epoch time", "time left");

        // This is so we can nicely align the output of the runs list
        let run_id_width = rows
            .iter()
            .map(|(id, ..)| id.len())
            .max()
            .unwrap_or(0)
            .max(headers.0.len());
        let state_width = rows
            .iter()
            .map(|(_, st, ..)| st.len())
            .max()
            .unwrap_or(0)
            .max(headers.1.len());
        let clients_width = rows
            .iter()
            .map(|(_, _, cl, ..)| cl.len())
            .max()
            .unwrap_or(0)
            .max(headers.2.len());
        let epoch_width = rows
            .iter()
            .map(|(_, _, _, ep, _)| ep.len())
            .max()
            .unwrap_or(0)
            .max(headers.3.len());

        let mut result = vec![format!(
            "  {:<run_id_width$}   {:<state_width$}   {:<clients_width$}   {:<epoch_width$}   {}",
            headers.0, headers.1, headers.2, headers.3, headers.4
        )];

        result.extend(rows.iter().map(|(run_id, state, clients, epoch, remaining)| {
            format!(
                "  {:<run_id_width$}   {:<state_width$}   {:<clients_width$}   {:<epoch_width$}   {}",
                run_id, state, clients, epoch, remaining
            )
        }));

        result
    }
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
            let Ok(instance) = CoordinatorInstance::try_deserialize(&mut account.data.as_slice())
            else {
                debug!("Failed to deserialize CoordinatorInstance at {}", pubkey);
                continue;
            };

            let Ok(coord_account) = self.rpc_client.get_account(&instance.coordinator_account)
            else {
                debug!(
                    "Skipping run {} - could not fetch coordinator account",
                    instance.run_id
                );
                continue;
            };

            let Ok(coordinator) = coordinator_account_from_bytes(&coord_account.data) else {
                debug!(
                    "Skipping run {} - could not deserialize coordinator account",
                    instance.run_id
                );
                continue;
            };

            let state = &coordinator.state.coordinator;
            runs.push(RunInfo {
                run_id: instance.run_id.clone(),
                instance_pubkey: pubkey,
                coordinator_account: instance.coordinator_account,
                run_state: state.run_state,
                num_clients: state.epoch_state.clients.len(),
                min_clients: state.config.min_clients,
                epoch_time_secs: state.config.epoch_time,
                epoch_start_timestamp: state.epoch_state.start_timestamp,
            });
        }

        Ok(runs)
    }

    /// Check if a user is authorized to join a specific run.
    ///
    /// This checks permissionless authorization (grantee = system_program::ID),
    /// user-specific authorization (grantee = user_pubkey),
    /// and optionally delegate-key authorization.
    /// Returns the matched grantee pubkey if authorized, or None if not.
    pub fn can_user_join_run(
        &self,
        run_id: &str,
        user_pubkey: &Pubkey,
        delegate_authorizer: Option<&Pubkey>,
    ) -> Result<Option<Pubkey>> {
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

        // Try delegate-key authorization if provided
        if let Some(authorizer) = delegate_authorizer {
            debug!("Attempting authorization via delegate key {}", authorizer);
            if self.check_authorization_for_grantee(&join_authority, authorizer, user_pubkey) {
                return Ok(Some(*authorizer));
            }
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
