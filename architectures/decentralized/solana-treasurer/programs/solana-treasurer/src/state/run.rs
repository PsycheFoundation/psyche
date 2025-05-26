use anchor_lang::prelude::*;

#[account()]
#[derive(Debug)]
pub struct Run {
    pub bump: u8,
    pub index: u64,

    pub main_authority: Pubkey,
    pub join_authority: Pubkey,

    pub coordinator_account: Pubkey,
    pub coordinator_instance: Pubkey,

    pub collateral_mint: Pubkey,
    pub collateral_amount_per_earned_point: u64,

    pub total_funded_collateral_amount: u64,
    pub total_claimed_collateral_amount: u64,
    pub total_claimed_earned_points: u64,
}

impl Run {
    pub const SEEDS_PREFIX: &'static [u8] = b"Run";

    pub fn space_with_discriminator() -> usize {
        8 + std::mem::size_of::<Run>()
    }
}
