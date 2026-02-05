use anchor_lang::prelude::*;
use bytemuck::Pod;
use bytemuck::Zeroable;
use psyche_coordinator::ClientState;
use psyche_coordinator::Coordinator;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::CoordinatorProgress;
use psyche_coordinator::HealthChecks;
use psyche_coordinator::RunState;
use psyche_coordinator::SOLANA_MAX_STRING_LEN;
use psyche_coordinator::TickResult;
use psyche_coordinator::Witness;
use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::model::Model;
use psyche_core::FixedString;
use psyche_core::SmallBoolean;
use psyche_core::sha256v;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

use crate::ClientId;
use crate::ProgramError;
use crate::client::Client;
use crate::clients_state::ClientsState;

#[derive(
    Debug,
    Clone,
    Copy,
    Zeroable,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    TS,
    Default,
    PartialEq,
)]
#[repr(C)]
pub struct RunMetadata {
    pub name: FixedString<{ SOLANA_MAX_STRING_LEN }>,

    pub description: FixedString<280>,

    pub num_parameters: u64,
    pub vocab_size: u64,
}

impl RunMetadata {}

#[derive(
    Debug,
    Clone,
    Copy,
    Zeroable,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    TS,
)]
#[repr(C)]
pub struct CoordinatorInstanceState {
    pub metadata: RunMetadata,
    pub coordinator: Coordinator<ClientId>,
    pub clients_state: ClientsState,
    pub is_warmup_first_tick: SmallBoolean,
    pub is_training_first_tick: SmallBoolean,
    pub client_version: FixedString<96>,
}

unsafe impl Pod for CoordinatorInstanceState {}

impl CoordinatorInstanceState {
    fn get_random_seed(clock: &Clock) -> u64 {
        let random_seed_bytes = sha256v(&[
            &clock.unix_timestamp.to_ne_bytes(),
            &clock.slot.to_ne_bytes(),
        ]);

        let mut random_seed: [u8; 8] = [0; 8];
        random_seed.copy_from_slice(&random_seed_bytes[..8]);
        u64::from_ne_bytes(random_seed)
    }

    pub fn tick(&mut self) -> Result<()> {
        let active_clients_ids = match self.coordinator.run_state {
            RunState::WaitingForMembers => {
                // Reset state flags
                self.is_warmup_first_tick = SmallBoolean::from(true);
                self.is_training_first_tick = SmallBoolean::from(true);

                let active_clients_ids =
                    self.clients_state.get_active_clients_ids();
                msg!(
                    "Pending active clients ids: {}",
                    active_clients_ids.len()
                );
                Some(active_clients_ids)
            },
            _ => None,
        };

        msg!("Pre-tick run state: {}", self.coordinator.run_state);

        let clock: Clock = Clock::get()?;
        match self.coordinator.tick(
            active_clients_ids,
            clock.unix_timestamp as u64,
            Self::get_random_seed(&clock),
        ) {
            Ok(TickResult::Ticked) => {
                if self.coordinator.is_warmup_just_starting()
                    && self.is_warmup_first_tick.is_true()
                {
                    msg!("New epoch just starting, save epoch rewards rate");
                    self.clients_state.current_epoch_rates =
                        self.clients_state.future_epoch_rates;
                    self.is_warmup_first_tick = SmallBoolean::from(false);
                } else if self.coordinator.is_training_just_starting()
                    && self.is_training_first_tick.is_true()
                {
                    msg!("New epoch just starting, save epoch active clients");
                    self.clients_state.next_active += 1;
                    self.is_training_first_tick = SmallBoolean::from(false);
                }
            },
            Ok(TickResult::EpochEnd(success)) => {
                msg!("Epoch end, sucecsss: {}", success);

                let finished_clients = &self.coordinator.epoch_state.clients;
                let exited_clients =
                    &self.coordinator.epoch_state.exited_clients;

                let mut finished_client_index = 0;
                let mut exited_client_index = 0;

                for client in self.clients_state.clients.iter_mut() {
                    if finished_client_index < finished_clients.len()
                        && client.id.signer
                            == finished_clients[finished_client_index].id.signer
                    {
                        if finished_clients[finished_client_index].state
                            == ClientState::Healthy
                        {
                            client.earned += self
                                .clients_state
                                .current_epoch_rates
                                .earning_rate_total_shared
                                .saturating_div(finished_clients.len() as u64);
                        }
                        finished_client_index += 1;
                    }
                    if exited_client_index < exited_clients.len()
                        && client.id.signer
                            == exited_clients[exited_client_index].id.signer
                    {
                        if exited_clients[exited_client_index].state
                            == ClientState::Ejected
                        {
                            client.slashed += self
                                .clients_state
                                .current_epoch_rates
                                .slashing_rate_per_client;
                        }
                        exited_client_index += 1;
                    }
                }
            },
            Err(err) => return err!(ProgramError::from(err)),
        };

        msg!("Post-tick run state: {}", self.coordinator.run_state);
        Ok(())
    }

    pub fn set_paused(&mut self, paused: bool) -> Result<()> {
        let unix_timestamp = Clock::get()?.unix_timestamp as u64;
        msg!(
            "set_paused called: paused={}, state={}",
            paused,
            self.coordinator.run_state
        );
        if let Err(err) = match paused {
            true => self.coordinator.pause(unix_timestamp),
            false => {
                if !self.coordinator.config.check() {
                    return err!(ProgramError::ConfigSanityCheckFailed);
                }
                if !self.coordinator.model.check() {
                    return err!(ProgramError::ModelSanityCheckFailed);
                }

                if self.coordinator.run_state == RunState::Uninitialized {
                    // this is the only way to get out of RunState::Uninitialized
                    // by doing this we force the sanity checks on the config and model
                    // pass before starting the first step
                    self.coordinator.run_state = RunState::Paused;
                    // step 1 is the first valid step
                    self.coordinator.progress.step = 1;
                }

                // check progress after potentially setting step to 1 (errors for default 0)
                if !self.coordinator.progress.check() {
                    return err!(ProgramError::ProgressSanityCheckFailed);
                }

                self.coordinator.resume(unix_timestamp)
            },
        } {
            return err!(ProgramError::from(err));
        }

        if paused {
            Ok(()) // do not tick when setting paused, tick() errors when paused
        } else {
            // clear all active joins -- require that everyone re-join
            self.clients_state.next_active += 1;

            self.tick()
        }
    }

    pub fn witness(&mut self, payer: &Pubkey, witness: Witness) -> Result<()> {
        let id = self.clients_state.find_signer(payer)?;

        let clock: Clock = Clock::get()?;
        self.coordinator
            .witness(id, witness, clock.unix_timestamp as u64)
            .map_err(|err| anchor_lang::error!(ProgramError::from(err)))?;

        self.tick()
    }

    pub fn cooldown_witness(
        &mut self,
        payer: &Pubkey,
        witness: Witness,
    ) -> Result<()> {
        let id = self.clients_state.find_signer(payer)?;

        self.coordinator
            .cooldown_witness(id, witness)
            .map_err(|err| anchor_lang::error!(ProgramError::from(err)))?;

        self.tick()
    }

    pub fn warmup_witness(
        &mut self,
        payer: &Pubkey,
        witness: Witness,
    ) -> Result<()> {
        let id = self.clients_state.find_signer(payer)?;

        let clock: Clock = Clock::get()?;
        self.coordinator
            .warmup_witness(
                id,
                witness,
                clock.unix_timestamp as u64,
                Self::get_random_seed(&clock),
            )
            .map_err(|err| anchor_lang::error!(ProgramError::from(err)))?;

        self.tick()
    }

    pub fn set_future_epoch_rates(
        &mut self,
        epoch_earning_rate_total_shared: Option<u64>,
        epoch_slashing_rate_per_client: Option<u64>,
    ) -> Result<()> {
        if let Some(epoch_earning_rate_total_shared) =
            epoch_earning_rate_total_shared
        {
            self.clients_state
                .future_epoch_rates
                .earning_rate_total_shared = epoch_earning_rate_total_shared;
        }
        if let Some(epoch_slashing_rate_per_client) =
            epoch_slashing_rate_per_client
        {
            self.clients_state
                .future_epoch_rates
                .slashing_rate_per_client = epoch_slashing_rate_per_client;
        }
        Ok(())
    }

    pub fn update(
        &mut self,
        metadata: Option<RunMetadata>,
        config: Option<CoordinatorConfig>,
        model: Option<Model>,
        progress: Option<CoordinatorProgress>,
    ) -> Result<()> {
        if self.coordinator.run_state == RunState::Finished {
            return err!(ProgramError::UpdateConfigFinished);
        } else if !self.coordinator.halted()
            // these can't be updated without pausing
            // but metadata can be updated without pausing so it's not included here
            && (config.is_some() || model.is_some() || progress.is_some())
        {
            return err!(ProgramError::UpdateConfigNotHalted);
        }

        if let Some(metadata) = metadata {
            let _ = std::mem::replace(&mut self.metadata, metadata);
        }

        if let Some(config) = config {
            if !config.check() {
                return err!(ProgramError::ConfigSanityCheckFailed);
            }

            let _ = std::mem::replace(&mut self.coordinator.config, config);
        }

        if let Some(model) = model {
            if !model.check() {
                return err!(ProgramError::ModelSanityCheckFailed);
            }

            let _ = std::mem::replace(&mut self.coordinator.model, model);
        }

        if let Some(progress) = progress {
            if !progress.check() {
                return err!(ProgramError::ModelSanityCheckFailed);
            }

            let _ = std::mem::replace(&mut self.coordinator.progress, progress);
        }

        Ok(())
    }

    pub fn join_run(&mut self, id: ClientId) -> Result<()> {
        let existing = match self
            .clients_state
            .clients
            .iter_mut()
            .find(|x| x.id.signer == id.signer)
        {
            Some(client) => {
                if client.id != id {
                    return err!(ProgramError::ClientIdMismatch);
                }
                client.id = id; // IMPORTANT. Equality is on wallet key but includes ephemeral p2p key
                client.active = self.clients_state.next_active;
                msg!("Existing client {} re-joined", id.signer);
                true
            },
            None => false,
        };

        if !existing {
            let total_num_clients = self.clients_state.clients.len() as u16;
            if self.coordinator.config.global_batch_size_end
                == total_num_clients
            {
                return err!(ProgramError::MoreClientsThanBatches);
            }

            let new_client = Client {
                id,
                earned: 0,
                slashed: 0,
                active: self.clients_state.next_active,
                _unused: Default::default(),
            };

            if self.clients_state.clients.push(new_client).is_err() {
                return err!(ProgramError::ClientsFull);
            }
            msg!(
                "New client {} joined, {} total clients",
                id.signer,
                self.clients_state.clients.len()
            );
        }

        if !self.coordinator.halted() {
            self.tick()
        } else {
            Ok(())
        }
    }

    pub fn health_check(
        &mut self,
        payer: &Pubkey,
        checks: HealthChecks<ClientId>,
    ) -> Result<()> {
        // O(n) on clients, reconsider
        let id = self.clients_state.find_signer(payer)?;

        self.coordinator
            .health_check(id, checks)
            .map_err(|err| anchor_lang::error!(ProgramError::from(err)))?;
        self.tick()
    }

    pub fn checkpoint(
        &mut self,
        payer: &Pubkey,
        repo: Checkpoint,
    ) -> Result<()> {
        // O(n) on clients, reconsider
        let id = self.clients_state.find_signer(payer)?;
        let index = self
            .coordinator
            .epoch_state
            .clients
            .iter()
            .position(|x| x.id == *id)
            .ok_or(ProgramError::SignerNotAClient)?;

        self.coordinator
            .checkpoint(id, index as u64, repo)
            .map_err(|err| anchor_lang::error!(ProgramError::from(err)))?;

        // Only tick if not halted (Paused/Uninitialized/Finished)
        // Checkpoint update itself doesn't require state transitions
        if !self.coordinator.halted() {
            self.tick()
        } else {
            msg!(
                "Checkpoint recorded while halted (state: {}), skipping tick",
                self.coordinator.run_state
            );
            Ok(())
        }
    }
}
