use std::cmp::max;

use crate::{Coordinator, CoordinatorError, coordinator::SOLANA_MAX_NUM_CHECKPOINTERS};
use psyche_core::{NodeIdentity, compute_shuffled_index, sha256, sha256v};

use super::types::salts;

#[derive(Clone)]
pub struct CheckpointerSelection {
    checkpointers: u64,
    seed: [u8; 32],
}

impl CheckpointerSelection {
    pub fn new(checkpointers: u64, seed: [u8; 32]) -> Self {
        Self {
            checkpointers,
            seed,
        }
    }

    pub fn from_coordinator<T: NodeIdentity>(
        coordinator: &Coordinator<T>,
        offset: isize,
    ) -> Result<Self, CoordinatorError> {
        let round = get_round_by_offset(coordinator, offset)?;
        let seed = sha256(&round.random_seed.to_le_bytes());

        let checkpointers = max(
            (coordinator.epoch_state.clients.len() / 3).min(SOLANA_MAX_NUM_CHECKPOINTERS),
            1,
        ) as u64;
        Ok(Self {
            checkpointers,
            seed,
        })
    }

    pub fn is_checkpointer(&self, client_index: u64, total_clients: u64) -> bool {
        let final_seed = compute_salted_seed(&self.seed, salts::COOLDOWN);
        let index = compute_shuffled_index(client_index, total_clients, &final_seed);
        index < self.checkpointers
    }
}

pub(crate) fn get_round_by_offset<T: NodeIdentity>(
    coordinator: &Coordinator<T>,
    offset: isize,
) -> Result<&crate::Round, CoordinatorError> {
    match offset {
        -2 => coordinator.previous_previous_round(),
        -1 => coordinator.previous_round(),
        0 => coordinator.current_round(),
        _ => return Err(CoordinatorError::NoActiveRound),
    }
    .ok_or(CoordinatorError::NoActiveRound)
}

pub(crate) fn compute_salted_seed(seed: &[u8; 32], salt: &str) -> [u8; 32] {
    let mut result = [0u8; 32];
    result.copy_from_slice(&sha256v(&[&sha256(seed), salt.as_bytes()]));
    result
}
