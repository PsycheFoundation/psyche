use anchor_lang::prelude::*;

#[account()]
#[derive(Debug, InitSpace)]
pub struct Claim {
    pub bump: u8,

    pub claimed_collateral_amount: u64,
}

impl Claim {
    pub const SEEDS_PREFIX: &'static [u8] = b"Claim";

    pub fn space_with_discriminator() -> usize {
        8 + Claim::INIT_SPACE
    }
}
