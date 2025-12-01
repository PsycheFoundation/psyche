use psyche_solana_distributor::state;
use solana_sdk::pubkey::Pubkey;

pub fn find_pda_airdrop(airdrop_id: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[state::Airdrop::SEEDS_PREFIX, &airdrop_id.to_le_bytes()],
        &psyche_solana_distributor::ID,
    )
    .0
}

pub fn find_pda_claim(
    airdrop: &Pubkey,
    claimer: &Pubkey,
    nonce: u64,
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            state::Claim::SEEDS_PREFIX,
            airdrop.as_ref(),
            claimer.as_ref(),
            nonce.to_le_bytes().as_ref(),
        ],
        &psyche_solana_distributor::ID,
    )
    .0
}
