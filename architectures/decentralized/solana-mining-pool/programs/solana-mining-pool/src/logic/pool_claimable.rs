use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

use crate::state::Pool;
use crate::ProgramError;

#[derive(Accounts)]
#[instruction(params: PoolClaimableParams)]
pub struct PoolClaimableAccounts<'info> {
    #[account()]
    pub authority: Signer<'info>,

    #[account(
        mut,
        constraint = pool.authority == authority.key(),
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account()]
    pub redeemable_mint: Box<Account<'info, Mint>>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct PoolClaimableParams {}

pub fn pool_claimable_processor(
    context: Context<PoolClaimableAccounts>,
    _params: PoolClaimableParams,
) -> Result<()> {
    let pool = &mut context.accounts.pool;

    if pool.freeze {
        return err!(ProgramError::PoolFreezeIsTrue);
    }
    if pool.claiming_enabled {
        return err!(ProgramError::PoolClaimingEnabledIsTrue);
    }

    pool.claiming_enabled = true;
    pool.redeemable_mint = context.accounts.redeemable_mint.key();
    pool.total_claimed_redeemable_amount = 0;

    Ok(())
}
