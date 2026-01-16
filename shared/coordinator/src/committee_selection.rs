use crate::{Client, Coordinator, CoordinatorError, SOLANA_MAX_NUM_WITNESSES};

use anchor_lang::{AnchorDeserialize, AnchorSerialize, InitSpace, prelude::borsh};
use bytemuck::Zeroable;
use psyche_core::{NodeIdentity, SmallBoolean, compute_shuffled_index, sha256, sha256v};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const COMMITTEE_SALT: &str = "committee";
pub const WITNESS_SALT: &str = "witness";

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
)]
#[repr(C)]
pub enum Committee {
    #[default]
    TieBreaker,
    Verifier,
    Trainer,
}

#[derive(Clone)]
pub struct CommitteeSelection {
    tie_breaker_nodes: u64,
    verifier_nodes: u64,
    total_nodes: u64,
    witness_nodes: u64,
    seed: [u8; 32],
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
)]
#[repr(C)]
pub struct CommitteeProof {
    pub committee: Committee,
    pub position: u64,
    pub index: u64,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Zeroable,
    Default,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
    TS,
)]
#[repr(C)]
pub struct WitnessProof {
    // position in virtual shuffle, as determined by seed
    pub position: u64,
    // index into epoch_state.clients of sender
    pub index: u64,
    // assertion of witness membership or non-membership
    pub witness: SmallBoolean,
}

impl CommitteeSelection {
    pub fn new(
        tie_breaker_nodes: usize,
        witness_nodes: usize,
        verification_percent: u8,
        total_nodes: usize,
        seed: u64,
    ) -> Result<Self, CoordinatorError> {
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

    pub fn from_coordinator<T: NodeIdentity>(
        coordinator: &Coordinator<T>,
        offset: isize,
    ) -> Result<Self, CoordinatorError> {
        let round = match offset {
            -2 => coordinator.previous_previous_round(),
            -1 => coordinator.previous_round(),
            0 => coordinator.current_round(),
            _ => {
                return Err(CoordinatorError::NoActiveRound);
            }
        }
        .ok_or(CoordinatorError::NoActiveRound)?;
        Self::new(
            round.tie_breaker_tasks as usize,
            coordinator.config.witness_nodes as usize,
            coordinator.config.verification_percent,
            round.clients_len as usize,
            round.random_seed,
        )
    }

    pub fn get_witness(&self, index: u64) -> WitnessProof {
        let position = self.compute_shuffled_index(index, WITNESS_SALT);
        let witness = self.get_witness_from_position(position);
        WitnessProof {
            witness: witness.into(),
            position,
            index,
        }
    }

    pub fn get_committee(&self, index: u64) -> CommitteeProof {
        let position = self.compute_shuffled_index(index, COMMITTEE_SALT);
        let committee = self.get_committee_from_position(position);
        CommitteeProof {
            committee,
            position,
            index,
        }
    }

    pub fn get_committee_from_position(&self, committee_position: u64) -> Committee {
        if committee_position < self.tie_breaker_nodes {
            Committee::TieBreaker
        } else if committee_position < self.tie_breaker_nodes + self.verifier_nodes {
            Committee::Verifier
        } else {
            Committee::Trainer
        }
    }

    fn get_witness_from_position(&self, witness_position: u64) -> bool {
        match self.witness_nodes {
            0 => witness_position < SOLANA_MAX_NUM_WITNESSES as u64,
            witness_nodes => witness_position < witness_nodes,
        }
    }

    pub fn verify_committee_for_client<T: NodeIdentity>(
        &self,
        client_id: &T,
        proof: &CommitteeProof,
        clients: &[Client<T>],
    ) -> bool {
        Self::verify_client(client_id, proof.index, clients) && self.verify_committee(proof)
    }

    pub fn verify_witness_for_client<T: NodeIdentity>(
        &self,
        client_id: &T,
        proof: &WitnessProof,
        clients: &[Client<T>],
    ) -> bool {
        Self::verify_client(client_id, proof.index, clients) && self.verify_witness(proof)
    }

    fn verify_client<T: NodeIdentity>(client_id: &T, index: u64, clients: &[Client<T>]) -> bool {
        clients.get(index as usize).map(|c| &c.id) == Some(client_id)
    }

    fn verify_committee(&self, proof: &CommitteeProof) -> bool {
        let position = self.compute_shuffled_index(proof.index, COMMITTEE_SALT);
        proof.position == position && proof.committee == self.get_committee_from_position(position)
    }

    fn verify_witness(&self, proof: &WitnessProof) -> bool {
        let position = self.compute_shuffled_index(proof.index, WITNESS_SALT);
        proof.position == position
            && proof.witness == self.get_witness_from_position(position).into()
    }

    fn compute_shuffled_index(&self, index: u64, salt: &str) -> u64 {
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&sha256v(&[&self.seed, salt.as_bytes()]));

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

impl std::fmt::Display for Committee {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Committee::TieBreaker => write!(f, "Tie breaker"),
            Committee::Verifier => write!(f, "Verifier"),
            Committee::Trainer => write!(f, "Trainer"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_committee_selection() {
        let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();
        assert_eq!(cs.tie_breaker_nodes, 10);
        assert_eq!(cs.witness_nodes, 20);
        assert_eq!(cs.verifier_nodes, 27); // (100 - 10) * 30% = 27
        assert_eq!(cs.total_nodes, 100);
    }

    #[test]
    fn test_get_committee() {
        let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();

        // Test for all possible indexes
        for i in 0..100 {
            let proof = cs.get_committee(i);
            assert!(proof.position < 100);

            // Verify that the committee matches the position
            match proof.committee {
                Committee::TieBreaker => assert!(proof.position < 10),
                Committee::Verifier => assert!(proof.position >= 10 && proof.position < 37),
                Committee::Trainer => assert!(proof.position >= 37),
            }
        }
    }

    #[test]
    fn test_get_witness() {
        let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();

        // Test for all possible indexes
        for i in 0..100 {
            let proof = cs.get_witness(i);
            assert!(proof.position < 100);

            // Verify that the witness status matches the position
            if proof.witness.is_true() {
                assert!(proof.position < 20);
            } else {
                assert!(proof.position >= 20);
            }
        }
    }

    #[test]
    fn test_verify_committee() {
        let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();

        for i in 0..100 {
            let proof = cs.get_committee(i);
            assert!(cs.verify_committee(&proof));

            // Test with incorrect proof
            let incorrect_proof = CommitteeProof {
                committee: Committee::Verifier,
                position: 99,
                index: i,
            };
            assert!(!cs.verify_committee(&incorrect_proof));
        }
    }

    #[test]
    fn test_verify_witness() {
        let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();

        for i in 0..100 {
            let proof = cs.get_witness(i);
            assert!(cs.verify_witness(&proof));

            // Test with incorrect proof
            let incorrect_proof = WitnessProof {
                witness: !proof.witness,
                position: 99,
                index: i,
            };
            assert!(!cs.verify_witness(&incorrect_proof));
        }
    }

    #[test]
    fn test_committee_distribution() {
        let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();
        let mut tie_breaker_count = 0;
        let mut verifier_count = 0;
        let mut trainer_count = 0;

        for i in 0..100 {
            match cs.get_committee(i).committee {
                Committee::TieBreaker => tie_breaker_count += 1,
                Committee::Verifier => verifier_count += 1,
                Committee::Trainer => trainer_count += 1,
            }
        }

        assert_eq!(tie_breaker_count, 10);
        assert_eq!(verifier_count, 27);
        assert_eq!(trainer_count, 63);
    }

    #[test]
    fn test_witness_distribution() {
        let cs = CommitteeSelection::new(10, 20, 30, 100, 12345).unwrap();
        let mut witness_count = 0;

        for i in 0..100 {
            if cs.get_witness(i).witness.is_true() {
                witness_count += 1;
            }
        }

        assert_eq!(witness_count, 20);
    }

    #[test]
    fn test_get_num_nodes() {
        let cs = CommitteeSelection::new(10, 5, 20, 100, 12345).unwrap();
        assert_eq!(cs.get_num_tie_breaker_nodes(), 10);
        assert_eq!(cs.get_num_verifier_nodes(), 18);
        assert_eq!(cs.get_num_trainer_nodes(), 72);
    }

    #[test]
    fn test_seed_consistency() {
        let cs1 = CommitteeSelection::new(10, 5, 20, 100, 12345).unwrap();
        let cs2 = CommitteeSelection::new(10, 5, 20, 100, 12345).unwrap();
        assert_eq!(cs1.get_seed(), cs2.get_seed());
    }

    #[test]
    fn test_invalid_total_nodes() {
        assert!(CommitteeSelection::new(10, 5, 20, 9, 12345).is_err());
    }

    #[test]
    fn test_invalid_committee_selections() {
        // verification_percent > 100
        assert!(CommitteeSelection::new(10, 5, 101, 100, 12345).is_err());
        // total_nodes < tie_breaker_nodes
        assert!(CommitteeSelection::new(10, 5, 101, 5, 12345).is_err());
        // total_nodes < witness_nodes
        assert!(CommitteeSelection::new(10, 50, 101, 11, 12345).is_err());
        // total_nodes >= u64::MAX
        assert!(CommitteeSelection::new(10, 50, 101, u64::MAX as usize, 12345).is_err());
    }

    #[test]
    fn test_edge_case_all_tie_breakers() {
        let cs = CommitteeSelection::new(100, 5, 20, 100, 12345).unwrap();
        for i in 0..100 {
            let committee = cs.get_committee(i).committee;
            assert_eq!(committee, Committee::TieBreaker);
        }
    }

    #[test]
    fn test_edge_case_no_verifiers() {
        let cs = CommitteeSelection::new(10, 5, 0, 100, 12345).unwrap();
        let mut tie_breaker_count = 0;
        let mut trainer_count = 0;
        for i in 0..100 {
            let committee = cs.get_committee(i).committee;
            match committee {
                Committee::TieBreaker => tie_breaker_count += 1,
                Committee::Trainer => trainer_count += 1,
                _ => panic!("Unexpected committee type"),
            }
        }
        assert_eq!(tie_breaker_count, 10);
        assert_eq!(trainer_count, 90);
    }
}
