use anchor_lang::prelude::*;

#[account()]
#[derive(Debug)]
pub struct Airdrop {
    pub bump: u8,

    pub index: u64,
    pub authority: Pubkey,
    pub proof_root: [u8; 32],

    pub collateral_mint: Pubkey,
    pub total_claimed_collateral_amount: u64,

    pub freeze: bool,

    pub metadata: AirdropMetadata,
}

pub type AirdropMerkleHash = [u8; 32];

#[derive(Debug, AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct AirdropMetadata {
    pub length: u16,
    pub bytes: [u8; AirdropMetadata::BYTES],
}

impl Airdrop {
    pub const SEEDS_PREFIX: &'static [u8] = b"Airdrop";

    pub fn space_with_discriminator() -> usize {
        8 + std::mem::size_of::<Airdrop>()
    }
}

impl AirdropMetadata {
    pub const BYTES: usize = 400;
}
