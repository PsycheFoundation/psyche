pub mod logic;
pub mod state;

use anchor_lang::prelude::*;
use logic::*;

declare_id!("GQEX84Laeg8JSJiiP5hL9L1vi3gGAMB3E6r1eWhf2fjS");

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

    pub fn airdrop_withdraw(
        context: Context<AirdropWithdrawAccounts>,
        params: AirdropWithdrawParams,
    ) -> Result<()> {
        airdrop_withdraw_processor(context, params)
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
    #[msg("airdrop.claim_freeze is true")]
    AirdropClaimFreezeIsTrue,
    #[msg("params.merkle_proof is invalid")]
    ParamsMerkleProofIsInvalid,
    #[msg("params.merkle_root is zeroed")]
    ParamsMerkleRootIsZeroed,
    #[msg("params.collateral_amount is too large")]
    ParamsCollateralAmountIsTooLarge,
    #[msg("Vesting elapsed seconds overflow")]
    VestingElapsedSecondsOverflow,
    #[msg("Vesting collateral amount overflow")]
    VestingNumeratorCollateralAmountOverflow,
    #[msg("Vesting collateral amount overflow")]
    VestingDenominatorCollateralAmountOverflow,
    #[msg("Vesting vested collateral amount overflow")]
    VestingVestedCollateralAmountOverflow,
    #[msg("Accounting updates overflow")]
    AccountingUpdatesOverflow,
}
