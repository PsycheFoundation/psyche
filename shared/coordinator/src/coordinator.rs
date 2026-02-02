use crate::{
    Commitment, Committee, CommitteeProof, CommitteeSelection, WitnessProof,
    model::{Checkpoint, HubRepo, Model},
};

use anchor_lang::{AnchorDeserialize, AnchorSerialize, InitSpace, prelude::borsh};
use bytemuck::{Pod, Zeroable};
use psyche_core::{Bloom, FixedString, FixedVec, MerkleRoot, NodeIdentity, SmallBoolean, sha256};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, hash::Hash};
use ts_rs::TS;

pub const SOLANA_MAX_STRING_LEN: usize = 64;
pub const SOLANA_MAX_URL_STRING_LEN: usize = 192;
pub const SOLANA_MAX_NUM_CLIENTS: usize = 256;
pub const SOLANA_MAX_NUM_WITNESSES: usize = 32;
// run_id must be at most 32 bytes because of PDA constraints
pub const SOLANA_RUN_ID_MAX_LEN: usize = 32;

pub const BLOOM_FALSE_RATE: f64 = 0.01f64;
pub const WITNESS_QUORUM_RAIO: f64 = 2.0f64 / 3.0f64;
pub const WAITING_FOR_MEMBERS_EXTRA_SECONDS: u64 = 10;
// max amount of tokens to send in a witness message
pub const MAX_TOKENS_TO_SEND: usize = 16;

// bloom filter with 1024 bits (16 u64)
pub type WitnessBloom = Bloom<16, 8>;

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Zeroable,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
    TS,
)]
#[repr(u8)]
pub enum RunState {
    #[default]
    Uninitialized = 0,
    WaitingForMembers = 1,
    Warmup = 2,
    RoundTrain = 3,
    RoundWitness = 4,
    Cooldown = 5,
    Finished = 6,
    Paused = 7,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Zeroable,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
    TS,
)]
#[repr(u8)]
pub enum ClientState {
    #[default]
    Healthy = 0,
    Dropped = 1,
    Withdrawn = 2,
    Ejected = 3,
}

#[derive(
    Clone,
    Debug,
    Zeroable,
    Default,
    Copy,
    Serialize,
    Deserialize,
    AnchorDeserialize,
    AnchorSerialize,
    TS,
)]
#[serde(bound = "I: NodeIdentity")]
#[repr(C)]
pub struct Client<I> {
    pub id: I,
    pub state: ClientState,
    pub exited_height: u32,
}

impl std::fmt::Display for ClientState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientState::Healthy => write!(f, "Healthy"),
            ClientState::Dropped => write!(f, "Dropped"),
            ClientState::Withdrawn => write!(f, "Withdrawn"),
            ClientState::Ejected => write!(f, "Ejected"),
        }
    }
}

impl<I: NodeIdentity> Hash for Client<I> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(
    Clone,
    Default,
    Debug,
    Zeroable,
    Copy,
    Serialize,
    Deserialize,
    AnchorSerialize,
    AnchorDeserialize,
    PartialEq,
    TS,
)]
#[repr(C)]
pub struct Round {
    pub witnesses: FixedVec<Witness, { SOLANA_MAX_NUM_WITNESSES }>,

    pub data_index: u64,
    pub random_seed: u64,
    pub height: u32,
    pub clients_len: u16,
    pub tie_breaker_tasks: u16,
}

#[derive(
    Clone,
    Debug,
    Zeroable,
    Default,
    Copy,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    PartialEq,
    TS,
)]
#[repr(C)]
pub struct Witness {
    pub proof: WitnessProof,
    pub participant_bloom: WitnessBloom,
    pub broadcast_bloom: WitnessBloom,
    pub broadcast_merkle: MerkleRoot,
}

#[derive(
    Clone,
    Copy,
    Zeroable,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    TS,
    Default,
    Debug,
)]
#[repr(C)]
pub struct WitnessMetadata {
    pub step: u32,
    pub tokens_per_sec: f32,
    pub bandwidth_per_sec: f32,
    pub loss: f32,
    pub evals: FixedVec<WitnessEvalResult, 8>,
    pub prompt_results: FixedVec<i32, { MAX_TOKENS_TO_SEND }>,
    pub prompt_index: u8,
    pub efficency: f32,
}

#[derive(
    Clone,
    Copy,
    Zeroable,
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    TS,
    Default,
    Debug,
)]
#[repr(C)]
pub struct WitnessEvalResult {
    pub name: FixedString<32>,
    pub value: f32,
}

impl WitnessEvalResult {
    pub fn new_trunc_name(name: &str, value: f32) -> Self {
        Self {
            name: FixedString::from_str_truncated(name),
            value,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum CoordinatorError {
    NoActiveRound,
    InvalidWitness,
    InvalidRunState,
    DuplicateWitness,
    InvalidHealthCheck,
    Halted,
    WitnessesFull,
    CannotResume,
    InvalidWithdraw,
    InvalidCommitteeSelection,
    InvalidCommitteeProof,
}

pub enum TickResult {
    Ticked,
    EpochEnd(bool), // if successfully finished
}

pub type HealthChecks<T> = Vec<(T, CommitteeProof)>;

pub const NUM_STORED_ROUNDS: usize = 4;

#[derive(
    Clone, Debug, Zeroable, Copy, Serialize, Deserialize, AnchorDeserialize, AnchorSerialize, TS,
)]
#[repr(C)]
pub struct CoordinatorConfig {
    pub warmup_time: u64,
    pub cooldown_time: u64,

    pub max_round_train_time: u64,
    pub round_witness_time: u64,
    pub global_batch_size_warmup_tokens: u64,

    pub epoch_time: u64,
    pub total_steps: u32,

    pub init_min_clients: u16,
    pub min_clients: u16,
    pub witness_nodes: u16,

    pub global_batch_size_start: u16,
    pub global_batch_size_end: u16,

    pub verification_percent: u8,
    pub waiting_for_members_extra_time: u8,
}

#[derive(
    Clone, Debug, Zeroable, Copy, Serialize, Deserialize, AnchorSerialize, AnchorDeserialize, TS,
)]
#[repr(C)]
#[serde(bound = "T: NodeIdentity")]
pub struct CoordinatorEpochState<T> {
    pub rounds: [Round; NUM_STORED_ROUNDS],
    /// **WARNING**: Using this can be a footgun:
    /// If you need to access the clients list for a particular round,
    /// e.g. when applying a message that could be from the previous round,
    /// This list might not be the list of clients at *that* round.
    /// Consider carefully if `get_client_at_historical_index` or
    /// `get_historical_clients` is what you actually want.
    pub clients: FixedVec<Client<T>, { SOLANA_MAX_NUM_CLIENTS }>,
    pub exited_clients: FixedVec<Client<T>, { SOLANA_MAX_NUM_CLIENTS }>,
    pub rounds_head: u32,
    pub start_step: u32,
    pub last_step: u32,
    pub start_timestamp: u64,
    pub first_round: SmallBoolean,
    pub cold_start_epoch: SmallBoolean,
}

#[derive(
    Clone, Debug, Zeroable, Copy, Serialize, Deserialize, AnchorSerialize, AnchorDeserialize, TS,
)]
#[repr(C)]
pub struct CoordinatorProgress {
    pub epoch: u16,
    pub step: u32,
    pub epoch_start_data_index: u64,
}

#[derive(
    Clone, Debug, Zeroable, Copy, Serialize, Deserialize, AnchorSerialize, AnchorDeserialize, TS,
)]
#[serde(bound = "T: NodeIdentity")]
#[repr(C)]
pub struct Coordinator<T> {
    pub run_id: FixedString<{ SOLANA_RUN_ID_MAX_LEN }>,

    pub run_state: RunState,

    pub model: Model,

    pub config: CoordinatorConfig,

    #[serde(default)]
    pub progress: CoordinatorProgress,

    #[serde(default)]
    pub epoch_state: CoordinatorEpochState<T>, // note, gets zeroed at the start of every epoch (not persistent through epochs)

    #[serde(default)]
    pub run_state_start_unix_timestamp: u64,

    #[serde(default)]
    pub pending_pause: SmallBoolean,
}

unsafe impl<T: NodeIdentity + Zeroable> Pod for Coordinator<T> {}

impl TryFrom<usize> for RunState {
    type Error = CoordinatorError;

    fn try_from(value: usize) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(RunState::Uninitialized),
            1 => Ok(RunState::WaitingForMembers),
            2 => Ok(RunState::Warmup),
            3 => Ok(RunState::RoundTrain),
            4 => Ok(RunState::RoundWitness),
            5 => Ok(RunState::Cooldown),
            6 => Ok(RunState::Finished),
            7 => Ok(RunState::Paused),
            _ => Err(CoordinatorError::InvalidRunState),
        }
    }
}

impl From<RunState> for usize {
    fn from(val: RunState) -> Self {
        match val {
            RunState::Uninitialized => 0,
            RunState::WaitingForMembers => 1,
            RunState::Warmup => 2,
            RunState::RoundTrain => 3,
            RunState::RoundWitness => 4,
            RunState::Cooldown => 5,
            RunState::Finished => 6,
            RunState::Paused => 7,
        }
    }
}

impl<T: NodeIdentity> AsRef<[u8]> for Client<T> {
    fn as_ref(&self) -> &[u8] {
        self.id.as_ref()
    }
}

impl<T: NodeIdentity> PartialEq for Client<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: NodeIdentity> Eq for Client<T> {}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinatorError::NoActiveRound => write!(f, "No active round"),
            CoordinatorError::InvalidWitness => write!(f, "Invalid witness"),
            CoordinatorError::InvalidRunState => write!(f, "Invalid run state"),
            CoordinatorError::DuplicateWitness => write!(f, "Duplicate witness"),
            CoordinatorError::InvalidHealthCheck => write!(f, "Invalid health check"),
            CoordinatorError::Halted => write!(f, "Halted"),
            CoordinatorError::WitnessesFull => write!(f, "Witnesses full"),
            CoordinatorError::CannotResume => write!(f, "Cannot resume"),
            CoordinatorError::InvalidWithdraw => write!(f, "Invalid withdraw"),
            CoordinatorError::InvalidCommitteeSelection => write!(f, "Invalid committee selection"),
            CoordinatorError::InvalidCommitteeProof => write!(f, "Invalid committee proof"),
        }
    }
}

impl std::error::Error for CoordinatorError {}

impl std::fmt::Display for RunState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunState::Uninitialized => write!(f, "Uninitialized"),
            RunState::WaitingForMembers => write!(f, "Waiting for members"),
            RunState::Warmup => write!(f, "Warmup"),
            RunState::RoundTrain => write!(f, "Training"),
            RunState::RoundWitness => write!(f, "Witness"),
            RunState::Cooldown => write!(f, "Cooldown"),
            RunState::Finished => write!(f, "Finished"),
            RunState::Paused => write!(f, "Paused"),
        }
    }
}

impl<T: NodeIdentity> Default for CoordinatorEpochState<T> {
    fn default() -> Self {
        Self {
            rounds: Default::default(),
            rounds_head: Default::default(),
            first_round: true.into(),
            clients: Default::default(),
            exited_clients: Default::default(),
            cold_start_epoch: false.into(),
            start_step: Default::default(),
            last_step: Default::default(),
            start_timestamp: Default::default(),
        }
    }
}

impl Default for CoordinatorProgress {
    fn default() -> Self {
        Self {
            epoch: Default::default(),
            step: 1,
            epoch_start_data_index: Default::default(),
        }
    }
}

impl<T: NodeIdentity> Client<T> {
    pub fn new(id: T) -> Self {
        Self {
            id,
            state: ClientState::Healthy,
            exited_height: 0,
        }
    }
}

impl<T: NodeIdentity> Coordinator<T> {
    pub fn tick<'a, 'b>(
        &'a mut self,
        new_clients: Option<impl ExactSizeIterator<Item = &'b T>>,
        unix_timestamp: u64,
        random_seed: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        match self.run_state {
            RunState::Uninitialized | RunState::Finished | RunState::Paused => {
                Err(CoordinatorError::Halted)
            }
            RunState::WaitingForMembers => {
                self.tick_waiting_for_members(new_clients, unix_timestamp)
            }
            RunState::Warmup => self.tick_warmup(unix_timestamp, random_seed),
            RunState::RoundTrain => self.tick_round_train(unix_timestamp),
            RunState::RoundWitness => self.tick_round_witness(unix_timestamp, random_seed),
            RunState::Cooldown => self.tick_cooldown(unix_timestamp),
        }
    }

    pub fn warmup_witness(
        &mut self,
        from: &T,
        witness: Witness,
        unix_timestamp: u64,
        random_seed: u64,
    ) -> std::result::Result<(), CoordinatorError> {
        if self.halted() {
            return Err(CoordinatorError::Halted);
        }

        // If we received a warmup witness but we already transitioned to the next state, we just ignore it.
        if matches!(self.run_state, RunState::RoundTrain) {
            return Ok(());
        }

        if !matches!(self.run_state, RunState::Warmup) {
            return Err(CoordinatorError::InvalidRunState);
        }

        let witness_nodes = if self.config.witness_nodes == 0 {
            self.epoch_state.clients.len().min(SOLANA_MAX_NUM_WITNESSES)
        } else {
            self.config.witness_nodes as usize
        };

        // Everyone can send a witness in the warmup phase so we don't need to check for the committee
        let round = self.current_round().unwrap();
        for witness in round.witnesses.iter() {
            if self.epoch_state.clients[witness.proof.index as usize].id == *from {
                return Err(CoordinatorError::DuplicateWitness);
            }
        }

        let round = self.current_round_mut_unchecked();
        round
            .witnesses
            .push(witness)
            .map_err(|_| CoordinatorError::WitnessesFull)?;

        if round.witnesses.len() == witness_nodes {
            self.start_round_train(unix_timestamp, random_seed, 0);
        }

        Ok(())
    }

    pub fn witness(
        &mut self,
        from: &T,
        witness: Witness,
        unix_timestamp: u64,
    ) -> std::result::Result<(), CoordinatorError> {
        if self.halted() {
            return Err(CoordinatorError::Halted);
        }

        let witness_nodes = if self.config.witness_nodes == 0 {
            self.epoch_state.clients.len().min(SOLANA_MAX_NUM_WITNESSES)
        } else {
            self.config.witness_nodes as usize
        };

        if !matches!(
            self.run_state,
            RunState::RoundWitness | RunState::RoundTrain,
        ) {
            return Err(CoordinatorError::InvalidRunState);
        }

        if !CommitteeSelection::from_coordinator(self, 0)?.verify_witness_for_client::<T>(
            from,
            &witness.proof,
            &self.epoch_state.clients,
        ) || witness.proof.witness.is_false()
        {
            return Err(CoordinatorError::InvalidWitness);
        }

        let round = self.current_round().unwrap();
        for witness in round.witnesses.iter() {
            if self.epoch_state.clients[witness.proof.index as usize].id == *from {
                return Err(CoordinatorError::DuplicateWitness);
            }
        }
        let round = self.current_round_mut_unchecked();
        round
            .witnesses
            .push(witness)
            .map_err(|_| CoordinatorError::WitnessesFull)?;

        if round.witnesses.len() == witness_nodes && !(self.run_state == RunState::RoundWitness) {
            self.change_state(unix_timestamp, RunState::RoundWitness);
        }
        Ok(())
    }

    pub fn health_check(
        &mut self,
        _from: &T,
        checks: HealthChecks<T>,
    ) -> std::result::Result<u32, CoordinatorError> {
        if self.halted() {
            return Err(CoordinatorError::Halted);
        }
        // only health check after pipeline has been filled
        if self
            .current_round()
            .ok_or(CoordinatorError::NoActiveRound)?
            .height
            < 2
        {
            return Err(CoordinatorError::InvalidHealthCheck);
        }
        for (id, proof) in &checks {
            if self.healthy(id, proof)? {
                return Err(CoordinatorError::InvalidHealthCheck);
            }
        }
        let mut dropped = 0;
        for (_id, proof) in &checks {
            let index = proof.index as usize;
            let client = &mut self.epoch_state.clients[index];
            if client.state == ClientState::Healthy {
                client.state = ClientState::Dropped;
                dropped += 1;
            }
        }
        // todo: reward `from` for `dropped` health checks
        Ok(dropped)
    }

    pub fn checkpoint(
        &mut self,
        from: &T,
        index: u64,
        checkpoint_repo: Checkpoint,
    ) -> std::result::Result<(), CoordinatorError> {
        let index = index as usize;
        if index >= self.epoch_state.clients.len() || self.epoch_state.clients[index].id != *from {
            return Err(CoordinatorError::InvalidCommitteeProof);
        }

        // TODO: In the case of more than one checkpointer, this will overwrite the checkpoint
        // with the last checkpointed one. We could instead have a vector of checkpoints to have
        // more download options.
        let Model::LLM(llm) = &mut self.model;
        match (&llm.checkpoint, checkpoint_repo) {
            // If current is P2P, wrap the new checkpoint in P2P
            (Checkpoint::P2P(_), Checkpoint::Hub(hub_repo)) => {
                llm.checkpoint = Checkpoint::P2P(hub_repo);
            }
            (Checkpoint::P2PGcs(_), Checkpoint::Gcs(gcs_repo)) => {
                llm.checkpoint = Checkpoint::P2PGcs(gcs_repo);
            }
            // If current is Hub, only accept Hub updates
            (Checkpoint::Hub(_), Checkpoint::Hub(hub_repo)) => {
                llm.checkpoint = Checkpoint::Hub(hub_repo);
            }
            // If current is Gcs, only accept Gcs updates
            (Checkpoint::Gcs(_), Checkpoint::Gcs(gcs_repo)) => {
                llm.checkpoint = Checkpoint::Gcs(gcs_repo);
            }
            (Checkpoint::P2PGcs(_), Checkpoint::Hub(hub_repo)) => {
                llm.checkpoint = Checkpoint::P2P(hub_repo);
            }
            (Checkpoint::P2P(_), Checkpoint::Gcs(gcs_repo)) => {
                llm.checkpoint = Checkpoint::P2PGcs(gcs_repo);
            }
            // Ignore other combinations
            _ => {}
        }

        Ok(())
    }

    pub fn withdraw(&mut self, index: u64) -> std::result::Result<(), CoordinatorError> {
        let index = index as usize;
        if index < self.epoch_state.clients.len() {
            let client = &mut self.epoch_state.clients[index];
            if client.state == ClientState::Healthy {
                client.state = ClientState::Withdrawn;
                return Ok(());
            }
        }
        Err(CoordinatorError::InvalidWithdraw)
    }

    pub fn withdraw_all(&mut self) {
        if !self.epoch_state.clients.is_empty() {
            let clients_max_index = self.epoch_state.clients.len() - 1;
            for client_index in 0..=clients_max_index {
                let _ = self.withdraw(client_index as u64); // we need to withdraw everyone, ignore error of already withdrawn
            }
        }
    }

    pub fn pause(&mut self, unix_timestamp: u64) -> std::result::Result<(), CoordinatorError> {
        if !self.halted() {
            if self.active() {
                self.pending_pause = true.into();
            } else {
                self.withdraw_all();
                self.change_state(unix_timestamp, RunState::Paused);
                self.epoch_state.cold_start_epoch = true.into();
            }
            Ok(())
        } else {
            Err(CoordinatorError::Halted)
        }
    }

    pub fn resume(&mut self, unix_timestamp: u64) -> Result<(), CoordinatorError> {
        if self.run_state != RunState::Paused {
            return Err(CoordinatorError::CannotResume);
        }
        self.start_waiting_for_members(unix_timestamp);
        Ok(())
    }

    pub fn healthy(&self, id: &T, proof: &CommitteeProof) -> Result<bool, CoordinatorError> {
        let round = self
            .previous_round()
            .ok_or(CoordinatorError::NoActiveRound)?;
        let index = proof.index;
        if index < round.clients_len as u64 {
            let client = self
                .get_client_at_historical_index(index as usize, round.clients_len)
                .ok_or(CoordinatorError::InvalidCommitteeProof)?;
            let selection = CommitteeSelection::from_coordinator(self, -1)?;
            if client.id != *id
                || !selection.verify_committee_for_client(
                    &client.id,
                    proof,
                    &self.epoch_state.clients,
                )
            {
                return Err(CoordinatorError::InvalidCommitteeProof);
            }
            match proof.committee {
                Committee::TieBreaker => todo!(),
                Committee::Verifier => todo!(),
                Committee::Trainer => self.trainer_healthy(&client.id),
            }
        } else {
            Err(CoordinatorError::InvalidCommitteeProof)
        }
    }

    pub fn witness_quorum(&self, num_witnesses: u16) -> u16 {
        let witness_nodes = match self.config.witness_nodes {
            0 => num_witnesses,
            witness_nodes => witness_nodes,
        };
        match witness_nodes {
            0 => unreachable!(),
            1 => 1,
            2 => 2,
            3 => 2,
            witness_nodes => ((witness_nodes as f64 * WITNESS_QUORUM_RAIO) as u16).max(1),
        }
    }

    pub fn trainer_healthy(&self, id: &T) -> Result<bool, CoordinatorError> {
        let prev_round_witnesses = &self
            .previous_round()
            .ok_or(CoordinatorError::NoActiveRound)?
            .witnesses;

        let score = Self::trainer_healthy_score_by_witnesses(id, prev_round_witnesses);
        Ok(score >= self.witness_quorum(prev_round_witnesses.len() as u16))
    }

    /// Computes the health score of a client based on witness confirmations.
    /// The score increases for each witness whose participant bloom filter contains the client's hashed ID.
    pub fn trainer_healthy_score_by_witnesses(id: &T, witnesses: &[Witness]) -> u16 {
        let hash = sha256(id.as_ref());

        let mut score = 0u16;
        for witness in witnesses {
            if witness.participant_bloom.contains(&hash) {
                score += 1;
            }
        }

        score
    }

    pub fn select_consensus_commitment_by_witnesses(
        commitments: &[Commitment],
        witnesses: &[Witness],
        witness_quorum: u16,
    ) -> Option<usize> {
        let mut scores = vec![0; commitments.len()];
        for witness in witnesses {
            for (index, commitment) in commitments.iter().enumerate() {
                if witness.broadcast_bloom.contains(&commitment.data_hash) {
                    scores[index] += 1;
                    break;
                }
            }
        }
        scores
            .into_iter()
            .enumerate()
            .filter(|(_, score)| *score >= witness_quorum)
            .max_by_key(|(_, score)| *score)
            .map(|(index, _)| index)
    }

    pub fn current_round(&self) -> Option<&Round> {
        self.epoch_state
            .rounds
            .get(self.epoch_state.rounds_head as usize)
    }

    pub fn current_round_mut(&mut self) -> Option<&mut Round> {
        self.epoch_state
            .rounds
            .get_mut(self.epoch_state.rounds_head as usize)
    }

    pub fn current_round_unchecked(&self) -> &Round {
        &self.epoch_state.rounds[self.epoch_state.rounds_head as usize]
    }

    pub fn current_round_mut_unchecked(&mut self) -> &mut Round {
        &mut self.epoch_state.rounds[self.epoch_state.rounds_head as usize]
    }

    pub fn previous_round(&self) -> Option<&Round> {
        match self.current_round() {
            Some(round) => match self.epoch_state.rounds_head == 0 && round.height == 0 {
                true => None,
                false => match self.epoch_state.rounds_head == 0 {
                    true => Some(&self.epoch_state.rounds[NUM_STORED_ROUNDS - 1]),
                    false => {
                        Some(&self.epoch_state.rounds[self.epoch_state.rounds_head as usize - 1])
                    }
                },
            },
            None => None,
        }
    }

    pub fn previous_previous_round(&self) -> Option<&Round> {
        match self.current_round() {
            Some(round) => match self.epoch_state.rounds_head == 0 && round.height <= 1 {
                true => None,
                false => match self.epoch_state.rounds_head {
                    0 => Some(&self.epoch_state.rounds[NUM_STORED_ROUNDS - 2]),
                    1 => Some(&self.epoch_state.rounds[NUM_STORED_ROUNDS - 1]),
                    n => Some(&self.epoch_state.rounds[n as usize - 2]),
                },
            },
            None => None,
        }
    }

    pub fn active(&self) -> bool {
        !matches!(
            self.run_state,
            RunState::WaitingForMembers
                | RunState::Warmup
                | RunState::Uninitialized
                | RunState::Finished
                | RunState::Paused
        )
    }

    pub fn halted(&self) -> bool {
        matches!(
            self.run_state,
            RunState::Uninitialized | RunState::Finished | RunState::Paused
        )
    }

    pub fn get_client_at_historical_index(
        &self,
        n: usize,
        prev_clients_len: u16,
    ) -> Option<&Client<T>> {
        if n < self.epoch_state.clients.len() {
            Some(&self.epoch_state.clients[n])
        } else if n < prev_clients_len as usize {
            let offset: usize = prev_clients_len as usize - n - 1;
            self.epoch_state.exited_clients.iter().rev().nth(offset)
        } else {
            None
        }
    }

    pub fn get_historical_clients(&self, clients_len: u16) -> Vec<&Client<T>> {
        (0..clients_len)
            .filter_map(|i| self.get_client_at_historical_index(i as usize, clients_len))
            .collect()
    }

    pub fn get_sequence_length(&self) -> u32 {
        match &self.model {
            Model::LLM(llm) => llm.max_seq_len,
        }
    }

    pub fn get_target_global_batch_size(&self, round: Option<&Round>) -> u16 {
        let tokens_processed = self.total_tokens_processed(round);
        self.config.get_batch_size(tokens_processed)
    }

    pub fn total_tokens_processed(&self, round: Option<&Round>) -> u64 {
        // if no round active yet (e.g., warmup), use epoch_start_data_index
        let current_data_start_index = round
            .map(|r| r.data_index)
            .unwrap_or(self.progress.epoch_start_data_index);

        current_data_start_index * self.get_sequence_length() as u64
    }

    pub fn get_cold_start_warmup_bounds(&self) -> Option<(u32, u32)> {
        let Model::LLM(llm) = &self.model;
        let cold_start_warmup_steps = llm.cold_start_warmup_steps;
        if self.epoch_state.cold_start_epoch.is_false() || cold_start_warmup_steps == 0 {
            return None;
        }
        Some((
            self.epoch_state.start_step,
            self.epoch_state.start_step + cold_start_warmup_steps,
        ))
    }

    fn get_global_batch_size_for_tokens(&self, tokens_processed: u64) -> u16 {
        self.config.get_batch_size(tokens_processed)
    }

    fn tick_waiting_for_members<'a, 'b>(
        &'a mut self,
        pending_clients: Option<impl ExactSizeIterator<Item = &'b T>>,
        unix_timestamp: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        let Some(pending_clients) = pending_clients else {
            return Ok(TickResult::Ticked);
        };

        if pending_clients.len() as u16 >= self.config.init_min_clients
            && self.check_timeout(
                unix_timestamp,
                self.config.waiting_for_members_extra_time as u64,
            )
        // This extra time allows for more clients to join even if the minimum number of clients is reached
        {
            // Make sure that all unhealthy clients are kicked at this point
            let height = self.current_round_unchecked().height;
            self.move_clients_to_exited(height);

            // Read the pending clients
            let mut pending_clients_ordered = Vec::with_capacity(pending_clients.len());
            let mut pending_clients_unordered = HashSet::with_capacity(pending_clients.len());
            for pending_client in pending_clients {
                pending_clients_ordered.push(pending_client);
                pending_clients_unordered.insert(pending_client);
            }

            // Ensure at least one client in the previous epoch is present in pending_clients for the new epoch.
            // If all clients are no longer present we need to use a Hub checkpoint since there
            // will be no peers for P2P sharing.
            let all_prev_clients_disconnected = !self
                .epoch_state
                .clients
                .iter()
                .any(|client| pending_clients_unordered.contains(&client.id));
            if all_prev_clients_disconnected {
                let Model::LLM(llm) = &mut self.model;
                match llm.checkpoint {
                    Checkpoint::P2P(hub_repo) => llm.checkpoint = Checkpoint::Hub(hub_repo),
                    Checkpoint::P2PGcs(gcs_repo) => llm.checkpoint = Checkpoint::Gcs(gcs_repo),
                    Checkpoint::P2PDummy => llm.checkpoint = Checkpoint::Dummy(HubRepo::dummy()),
                    _ => {}
                }
            }

            let cold_start_epoch = self.epoch_state.cold_start_epoch;
            bytemuck::write_zeroes(&mut self.epoch_state);
            self.epoch_state.first_round = true.into();
            self.epoch_state.cold_start_epoch = cold_start_epoch;
            self.epoch_state.start_step = self.progress.step;
            self.epoch_state.start_timestamp = unix_timestamp;
            self.epoch_state
                .clients
                .extend(
                    pending_clients_ordered
                        .into_iter()
                        .take(SOLANA_MAX_NUM_CLIENTS)
                        .map(|x| Client::new(*x)),
                )
                .unwrap();

            self.start_warmup(unix_timestamp);
        }

        Ok(TickResult::Ticked)
    }

    fn tick_warmup(
        &mut self,
        unix_timestamp: u64,
        random_seed: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        if self.check_timeout(unix_timestamp, self.config.warmup_time) {
            self.start_round_train(unix_timestamp, random_seed, 0);
        } else {
            self.move_clients_to_exited(0);
        }
        if (self.epoch_state.clients.len() as u16) < self.config.min_clients {
            self.start_waiting_for_members(unix_timestamp);
            Ok(TickResult::EpochEnd(false))
        } else {
            Ok(TickResult::Ticked)
        }
    }

    fn tick_round_train(
        &mut self,
        unix_timestamp: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        if self.check_timeout(unix_timestamp, self.config.max_round_train_time) {
            self.change_state(unix_timestamp, RunState::RoundWitness);
        }
        Ok(TickResult::Ticked)
    }

    fn tick_round_witness(
        &mut self,
        unix_timestamp: u64,
        random_seed: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        if self.check_timeout(unix_timestamp, self.config.round_witness_time) {
            // TODO: Punish idle witnesses
            self.epoch_state.first_round = false.into();
            self.progress.step += 1;
            let current_round = self.current_round_unchecked();
            let height = current_round.height;
            let num_witnesses = current_round.witnesses.len() as u16;
            self.move_clients_to_exited(height);

            // If there are not witnesses, then we can't distinguish from
            // the situation where only witness nodes disconnected or everyone
            // disconnected. We just set everyone to withdrawn state and change
            // to Cooldown.
            if num_witnesses == 0 {
                self.withdraw_all();
                self.start_cooldown(unix_timestamp);
                return Ok(TickResult::Ticked);
            }

            // Once the timeout for the whole epoch is reached, we set the last step as the current
            // step plus two.
            if self.check_epoch_timeout(unix_timestamp) && !self.epoch_state.last_step_set() {
                let last_step: u32 = self.progress.step + 2;
                // Just a sanity check to be sure the epoch doesn't end too early since we need
                // at least 4 rounds per epoch for overlapped pipeling
                if last_step >= 4 {
                    self.epoch_state.last_step = last_step;
                }
            }

            // We reached the last step of the epoch, we transition to Cooldown
            if self.epoch_state.last_step_set() && self.progress.step == self.epoch_state.last_step
            {
                self.start_cooldown(unix_timestamp);
                return Ok(TickResult::Ticked);
            }

            // If we don't reach the min number of clients or registered witnesses for the current round,
            // we change to Cooldown
            if self.epoch_state.clients.len() < self.config.min_clients as usize
                || num_witnesses < self.witness_quorum(num_witnesses)
                || self.progress.step >= self.config.total_steps
                || self.pending_pause.is_true()
            {
                self.start_cooldown(unix_timestamp);
                return Ok(TickResult::Ticked);
            }

            self.start_round_train(unix_timestamp, random_seed, 0);
        }
        Ok(TickResult::Ticked)
    }

    fn tick_cooldown(
        &mut self,
        unix_timestamp: u64,
    ) -> std::result::Result<TickResult, CoordinatorError> {
        if self.check_timeout(unix_timestamp, self.config.cooldown_time) {
            let last_round_batch_size = self.get_target_global_batch_size(self.current_round());
            self.progress.epoch_start_data_index =
                self.current_round_unchecked().data_index + last_round_batch_size as u64;
            self.progress.epoch += 1;

            let current_round = self.current_round_unchecked();
            let height = current_round.height;
            self.move_clients_to_exited(height);

            // we've completed an epoch, switch to P2P from now on
            let Model::LLM(llm) = &mut self.model;
            match llm.checkpoint {
                Checkpoint::Hub(hub_repo) => llm.checkpoint = Checkpoint::P2P(hub_repo),
                Checkpoint::Dummy(_) => llm.checkpoint = Checkpoint::P2PDummy,
                Checkpoint::Gcs(gcs_repo) => llm.checkpoint = Checkpoint::P2PGcs(gcs_repo),
                _ => {}
            }

            if self.pending_pause.is_true() {
                self.withdraw_all();
                self.change_state(unix_timestamp, RunState::Paused);
                self.pending_pause = false.into();
                self.epoch_state.cold_start_epoch = true.into();
            } else {
                self.start_waiting_for_members(unix_timestamp);
                self.epoch_state.cold_start_epoch = false.into();
            }

            Ok(TickResult::EpochEnd(true))
        } else {
            Ok(TickResult::Ticked)
        }
    }

    fn check_timeout(&self, unix_timestamp: u64, duration: u64) -> bool {
        self.run_state_start_unix_timestamp != unix_timestamp
            && unix_timestamp >= duration + self.run_state_start_unix_timestamp
    }

    fn check_epoch_timeout(&self, unix_timestamp: u64) -> bool {
        self.epoch_state.start_timestamp != unix_timestamp
            && unix_timestamp >= self.epoch_state.start_timestamp + self.config.epoch_time
    }

    fn start_cooldown(&mut self, unix_timestamp: u64) {
        self.current_round_mut_unchecked().witnesses.clear(); // clear witnesses for re-use in warmup
        self.change_state(unix_timestamp, RunState::Cooldown);
    }

    fn start_round_train(&mut self, unix_timestamp: u64, random_seed: u64, tie_breaker_tasks: u16) {
        let (next_rounds_head, next_height, next_data_index) =
            if self.epoch_state.first_round.into() {
                // very first round, don't increment -- just start here
                (0usize, 0u32, self.progress.epoch_start_data_index)
            } else {
                let prev_round = &self.epoch_state.rounds[self.epoch_state.rounds_head as usize];
                let prev_round_start_tokens =
                    prev_round.data_index * self.get_sequence_length() as u64;
                let prev_round_batch_size =
                    self.get_global_batch_size_for_tokens(prev_round_start_tokens);
                (
                    (self.epoch_state.rounds_head + 1) as usize % self.epoch_state.rounds.len(),
                    prev_round.height + 1,
                    prev_round.data_index + prev_round_batch_size as u64,
                )
            };
        let round = &mut self.epoch_state.rounds[next_rounds_head];
        self.epoch_state.rounds_head = next_rounds_head as u32;
        round.clients_len = self.epoch_state.clients.len() as u16;
        round.height = next_height;
        round.data_index = next_data_index;
        round.tie_breaker_tasks = tie_breaker_tasks;
        round.random_seed = random_seed;
        round.witnesses.clear();
        self.change_state(unix_timestamp, RunState::RoundTrain);
    }

    fn start_warmup(&mut self, unix_timestamp: u64) {
        self.change_state(unix_timestamp, RunState::Warmup);
    }

    fn start_waiting_for_members(&mut self, unix_timestamp: u64) {
        self.change_state(
            unix_timestamp,
            if self.progress.step < self.config.total_steps {
                RunState::WaitingForMembers
            } else {
                RunState::Finished
            },
        );
    }

    fn change_state(&mut self, unix_timestamp: u64, new_state: RunState) {
        assert!(self.run_state != new_state);
        self.run_state_start_unix_timestamp = unix_timestamp;
        self.run_state = new_state;
    }

    fn move_clients_to_exited(&mut self, height: u32) {
        // WARNING: O(n) on number of clients, need to refactor
        self.epoch_state.clients.retain(|x| {
            if x.state != ClientState::Healthy {
                self.epoch_state.exited_clients.push(*x).unwrap();
                self.epoch_state
                    .exited_clients
                    .last_mut()
                    .unwrap()
                    .exited_height = height;
                false
            } else {
                true
            }
        });
    }

    pub fn is_warmup_just_starting(&self) -> bool {
        self.epoch_state.first_round.is_true() && self.run_state == RunState::Warmup
    }

    pub fn is_training_just_starting(&self) -> bool {
        self.epoch_state.first_round.is_true() && self.run_state == RunState::RoundTrain
    }
}

impl<T> CoordinatorEpochState<T> {
    // When an epoch reaches its timeout, the last step is set as the
    // current step + 2. When last_step is set to 0, we assume it has not
    // been set.
    pub fn last_step_set(&self) -> bool {
        self.last_step != 0
    }
}

impl CoordinatorConfig {
    pub fn check(&self) -> bool {
        self.max_round_train_time != 0
            && self.round_witness_time != 0
            && self.min_clients != 0
            && self.init_min_clients >= self.min_clients
            && self.init_min_clients as usize <= SOLANA_MAX_NUM_CLIENTS
            && self.global_batch_size_start != 0
            && self.global_batch_size_end != 0
            && self.global_batch_size_end >= self.global_batch_size_start
            && self.total_steps != 0
            && self.witness_nodes <= self.min_clients
            && self.witness_nodes as usize <= SOLANA_MAX_NUM_WITNESSES
            && self.cooldown_time > 0
            && self.waiting_for_members_extra_time > 0
    }

    pub fn get_batch_size(&self, total_tokens_processed: u64) -> u16 {
        if total_tokens_processed >= self.global_batch_size_warmup_tokens {
            self.global_batch_size_end
        } else {
            let progress =
                total_tokens_processed as f64 / self.global_batch_size_warmup_tokens as f64;
            (self.global_batch_size_start as f64
                + (self.global_batch_size_end as f64 - self.global_batch_size_start as f64)
                    * progress)
                .round() as u16
        }
    }
}

impl CoordinatorProgress {
    pub fn check(&self) -> bool {
        self.step > 0
    }
}
