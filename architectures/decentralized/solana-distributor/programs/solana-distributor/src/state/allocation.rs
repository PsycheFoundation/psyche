use anchor_lang::prelude::*;

use crate::state::MerkleHash;
use crate::state::Vesting;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Allocation {
    pub receiver: Pubkey,
    pub vesting: Vesting,
}

impl Allocation {
    pub fn to_merkle_hash(&self) -> MerkleHash {
        MerkleHash::from_parts(&[
            self.receiver.as_ref(),
            &self.vesting.start_unix_timestamp.to_le_bytes(),
            &self.vesting.duration_seconds.to_le_bytes(),
            &self.vesting.end_collateral_amount.to_le_bytes(),
        ])
    }
}
