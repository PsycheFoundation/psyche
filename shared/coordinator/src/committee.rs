use crate::{Client, Coordinator, CoordinatorError, SOLANA_MAX_NUM_WITNESSES};
use psyche_core::{NodeIdentity, compute_shuffled_index, sha256};

use super::checkpointer::get_round_by_offset;
use super::types::{Committee, CommitteeProof, WitnessProof, salts};

#[derive(Clone)]
pub struct CommitteeSelection {
    pub(crate) tie_breaker_nodes: u64,
    pub(crate) verifier_nodes: u64,
    pub(crate) total_nodes: u64,
    pub(crate) witness_nodes: u64,
    pub(crate) seed: [u8; 32],
}

impl CommitteeSelection {
    pub fn new(
        tie_breaker_nodes: usize,
        witness_nodes: usize,
        verification_percent: u8,
        total_nodes: usize,
        seed: u64,
    ) -> Result<Self, CoordinatorError> {
        Self::validate_params(
            tie_breaker_nodes,
            witness_nodes,
            verification_percent,
            total_nodes,
        )?;

        let free_nodes = total_nodes - tie_breaker_nodes;
        let verifier_nodes = (free_nodes * verification_percent as usize) / 100;
        let seed = sha256(&seed.to_le_bytes());

        Ok(Self {
            tie_breaker_nodes: tie_breaker_nodes as u64,
            verifier_nodes: verifier_nodes as u64,
            total_nodes: total_nodes as u64,
            witness_nodes: witness_nodes as u64,
            seed,
        })
    }

    fn validate_params(
        tie_breaker_nodes: usize,
        witness_nodes: usize,
        verification_percent: u8,
        total_nodes: usize,
    ) -> Result<(), CoordinatorError> {
        if total_nodes >= u64::MAX as usize {
            return Err(CoordinatorError::InvalidCommitteeSelection);
        }
        if total_nodes < tie_breaker_nodes {
            return Err(CoordinatorError::InvalidCommitteeSelection);
        }
        if witness_nodes != 0 && total_nodes < witness_nodes {
            return Err(CoordinatorError::InvalidCommitteeSelection);
        }
        if verification_percent > 100 {
            return Err(CoordinatorError::InvalidCommitteeSelection);
        }
        Ok(())
    }

    pub fn from_coordinator<T: NodeIdentity>(
        coordinator: &Coordinator<T>,
        offset: isize,
    ) -> Result<Self, CoordinatorError> {
        let round = get_round_by_offset(coordinator, offset)?;
        Self::new(
            round.tie_breaker_tasks as usize,
            coordinator.config.witness_nodes as usize,
            coordinator.config.verification_percent,
            round.clients_len as usize,
            round.random_seed,
        )
    }

    pub fn get_witness(&self, index: u64) -> WitnessProof {
        let position = self.compute_shuffled_index(index, salts::WITNESS);
        let witness = self.is_witness_at_position(position);
        WitnessProof {
            witness: witness.into(),
            position,
            index,
        }
    }

    pub fn verify_witness(&self, proof: &WitnessProof) -> bool {
        let position = self.compute_shuffled_index(proof.index, salts::WITNESS);
        proof.position == position && proof.witness == self.is_witness_at_position(position).into()
    }

    pub fn verify_witness_for_client<T: NodeIdentity>(
        &self,
        client_id: &T,
        proof: &WitnessProof,
        clients: &[Client<T>],
    ) -> bool {
        Self::verify_client(client_id, proof.index, clients) && self.verify_witness(proof)
    }

    fn is_witness_at_position(&self, position: u64) -> bool {
        match self.witness_nodes {
            0 => position < SOLANA_MAX_NUM_WITNESSES as u64,
            witness_nodes => position < witness_nodes,
        }
    }

    pub fn get_committee(&self, index: u64) -> CommitteeProof {
        let position = self.compute_shuffled_index(index, salts::COMMITTEE);
        let committee = self.get_committee_from_position(position);
        CommitteeProof {
            committee,
            position,
            index,
        }
    }

    pub fn get_committee_from_position(&self, position: u64) -> Committee {
        if position < self.tie_breaker_nodes {
            Committee::TieBreaker
        } else if position < self.tie_breaker_nodes + self.verifier_nodes {
            Committee::Verifier
        } else {
            Committee::Trainer
        }
    }

    pub fn verify_committee(&self, proof: &CommitteeProof) -> bool {
        let position = self.compute_shuffled_index(proof.index, salts::COMMITTEE);
        proof.position == position && proof.committee == self.get_committee_from_position(position)
    }

    pub fn verify_committee_for_client<T: NodeIdentity>(
        &self,
        client_id: &T,
        proof: &CommitteeProof,
        clients: &[Client<T>],
    ) -> bool {
        Self::verify_client(client_id, proof.index, clients) && self.verify_committee(proof)
    }

    fn verify_client<T: NodeIdentity>(client_id: &T, index: u64, clients: &[Client<T>]) -> bool {
        clients.get(index as usize).map(|c| &c.id) == Some(client_id)
    }

    fn compute_shuffled_index(&self, index: u64, salt: &str) -> u64 {
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&psyche_core::sha256v(&[&self.seed, salt.as_bytes()]));
        compute_shuffled_index(index, self.total_nodes, &seed)
    }

    pub fn get_seed(&self) -> [u8; 32] {
        self.seed
    }

    pub fn get_num_tie_breaker_nodes(&self) -> u64 {
        self.tie_breaker_nodes
    }

    pub fn get_num_verifier_nodes(&self) -> u64 {
        self.verifier_nodes
    }

    pub fn get_num_trainer_nodes(&self) -> u64 {
        self.total_nodes - self.tie_breaker_nodes - self.verifier_nodes
    }
}
