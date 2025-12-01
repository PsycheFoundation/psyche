use psyche_solana_distributor::state;
use solana_sdk::pubkey::Pubkey;

pub fn find_pda_airdrop(airdrop_index: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[state::Airdrop::SEEDS_PREFIX, &airdrop_index.to_le_bytes()],
        &psyche_solana_distributor::ID,
    )
    .0
}

pub fn find_pda_claim(airdrop: &Pubkey, claimer: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            state::Claim::SEEDS_PREFIX,
            airdrop.as_ref(),
            claimer.as_ref(),
        ],
        &psyche_solana_distributor::ID,
    )
    .0
}
