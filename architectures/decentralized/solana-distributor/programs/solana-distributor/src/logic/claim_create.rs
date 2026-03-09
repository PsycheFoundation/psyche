use anchor_lang::prelude::*;

use crate::ProgramError;
use crate::state::Airdrop;
use crate::state::Claim;

#[derive(Accounts)]
#[instruction(params: ClaimCreateParams)]
pub struct ClaimCreateAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub claimer: Signer<'info>,

    #[account()]
    pub airdrop: Box<Account<'info, Airdrop>>,

    #[account(
        init,
        payer = payer,
        space = Claim::space_with_discriminator(),
        seeds = [
            Claim::SEEDS_PREFIX,
            airdrop.key().as_ref(),
            claimer.key().as_ref(),
            params.nonce.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub claim: Box<Account<'info, Claim>>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct ClaimCreateParams {
    pub nonce: u64,
}

pub fn claim_create_processor(
    context: Context<ClaimCreateAccounts>,
    _params: ClaimCreateParams,
) -> Result<()> {
    let airdrop = &context.accounts.airdrop;
    if airdrop.claim_freeze {
        return err!(ProgramError::AirdropClaimFreezeIsTrue);
    }

    let claim = &mut context.accounts.claim;
    claim.bump = context.bumps.claim;
    claim.claimed_collateral_amount = 0;

    Ok(())
}
