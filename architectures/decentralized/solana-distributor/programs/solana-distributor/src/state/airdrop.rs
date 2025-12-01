use anchor_lang::prelude::*;

use crate::state::MerkleHash;

#[account()]
#[derive(Debug)]
pub struct Airdrop {
    pub bump: u8,

    pub index: u64,
    pub authority: Pubkey,

    pub collateral_mint: Pubkey,
    pub total_claimed_collateral_amount: u64,

    pub freeze: bool,
    pub merkle_root: MerkleHash,
    pub metadata: AirdropMetadata,
}

impl Airdrop {
    pub const SEEDS_PREFIX: &'static [u8] = b"Airdrop";

    pub fn space_with_discriminator() -> usize {
        8 + std::mem::size_of::<Airdrop>()
    }
}

#[derive(Debug, AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq)]
pub struct AirdropMetadata {
    pub length: u16,
    pub bytes: [u8; AirdropMetadata::BYTES],
}

impl AirdropMetadata {
    pub const BYTES: usize = 400;
}
