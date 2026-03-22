use crate::coordinator::{Client, Coordinator, CoordinatorError, SOLANA_MAX_NUM_WITNESSES};
use crate::node_identity::NodeIdentity;
use crate::sha::{sha256, sha256v};
use crate::swap_or_not::compute_shuffled_index;

use super::checkpointer_selection::get_round_by_offset;
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

    pub fn from_coordinator(
        coordinator: &Coordinator,
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

    pub fn verify_witness_for_client(
        &self,
        client_id: &NodeIdentity,
        proof: &WitnessProof,
        clients: &[Client],
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

    pub fn verify_committee_for_client(
        &self,
        client_id: &NodeIdentity,
        proof: &CommitteeProof,
        clients: &[Client],
    ) -> bool {
        Self::verify_client(client_id, proof.index, clients) && self.verify_committee(proof)
    }

    fn verify_client(client_id: &NodeIdentity, index: u64, clients: &[Client]) -> bool {
        clients.get(index as usize).map(|c| &c.id) == Some(client_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Committee, CommitteeProof, WitnessProof};

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
    fn test_invalid_comittee_selections() {
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
