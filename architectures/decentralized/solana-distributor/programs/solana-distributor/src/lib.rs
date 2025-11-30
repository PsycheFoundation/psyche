pub mod logic;
pub mod state;

use anchor_lang::prelude::*;
use logic::*;

declare_id!("CQy5JKR2Lrm16pqSY5nkMaMYSazRk2aYx99pJDNGupR7");

pub fn find_airdrop(airdrop_index: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[state::Airdrop::SEEDS_PREFIX, &airdrop_index.to_le_bytes()],
        &crate::ID,
    )
    .0
}

pub fn find_claim(airdrop: &Pubkey, claimer: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            state::Claim::SEEDS_PREFIX,
            airdrop.as_ref(),
            claimer.as_ref(),
        ],
        &crate::ID,
    )
    .0
}

#[program]
pub mod psyche_solana_distributor {
    use super::*;

    pub fn airdrop_create(
        context: Context<AirdropCreateAccounts>,
        params: AirdropCreateParams,
    ) -> Result<()> {
        airdrop_create_processor(context, params)
    }

    pub fn airdrop_update(
        context: Context<AirdropUpdateAccounts>,
        params: AirdropUpdateParams,
    ) -> Result<()> {
        airdrop_update_processor(context, params)
    }

    pub fn claim_create(
        context: Context<ClaimCreateAccounts>,
        params: ClaimCreateParams,
    ) -> Result<()> {
        claim_create_processor(context, params)
    }

    pub fn claim_redeem(
        context: Context<ClaimRedeemAccounts>,
        params: ClaimRedeemParams,
    ) -> Result<()> {
        claim_redeem_processor(context, params)
    }
}

#[error_code]
pub enum ProgramError {
    #[msg("math overflow")]
    MathOverflow,
    #[msg("airdrop.freeze is true")]
    AirdropFreezeIsTrue,
    #[msg("params.metadata.length is too large")]
    ParamsMetadataLengthIsTooLarge,
    #[msg("params.merkle_proof is invalid")]
    ParamsMerkleProofIsInvalid,
    #[msg("params.collateral_amount is too large")]
    ParamsCollateralAmountIsTooLarge,
}
