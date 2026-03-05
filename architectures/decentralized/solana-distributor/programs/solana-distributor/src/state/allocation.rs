use anchor_lang::prelude::*;

use crate::state::MerkleHash;
use crate::state::Vesting;

#[derive(Debug, Clone, Copy)]
pub struct Allocation {
    pub claimer: Pubkey,
    pub nonce: u64,
    pub vesting: Vesting,
}

impl Allocation {
    pub fn to_merkle_hash(&self) -> MerkleHash {
        MerkleHash::from_parts(&[
            self.claimer.as_ref(),
            &self.nonce.to_le_bytes(),
            &self.vesting.start_unix_timestamp.to_le_bytes(),
            &self.vesting.duration_seconds.to_le_bytes(),
            &self.vesting.end_collateral_amount.to_le_bytes(),
        ])
    }
}
