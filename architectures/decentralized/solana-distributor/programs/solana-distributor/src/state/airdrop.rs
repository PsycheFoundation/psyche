use std::fmt;

use anchor_lang::prelude::*;

use crate::state::MerkleHash;

#[account()]
#[derive(Debug, InitSpace)]
pub struct Airdrop {
    pub bump: u8,

    pub id: u64,
    pub authority: Pubkey,

    pub collateral_mint: Pubkey,
    pub total_claimed_collateral_amount: u64,

    pub claim_freeze: bool,
    pub merkle_root: MerkleHash,
    pub metadata: AirdropMetadata,
}

impl Airdrop {
    pub const SEEDS_PREFIX: &'static [u8] = b"Airdrop";

    pub fn space_with_discriminator() -> usize {
        8 + Airdrop::INIT_SPACE
    }
}

#[derive(InitSpace, AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AirdropMetadata {
    pub bytes: [u8; AirdropMetadata::SIZE],
}

impl AirdropMetadata {
    pub const SIZE: usize = 300;
}

impl std::fmt::Debug for AirdropMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        let parts = self.bytes.iter().map(|b| format!("{:02X}", b));
        write!(f, "{}", parts.collect::<String>())
    }
}
