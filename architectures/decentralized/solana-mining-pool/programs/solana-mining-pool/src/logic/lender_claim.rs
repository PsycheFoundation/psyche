use anchor_lang::prelude::*;
use anchor_spl::token::Mint;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;
use anchor_spl::token::Transfer;
use anchor_spl::token::transfer;

use crate::ProgramError;
use crate::state::Lender;
use crate::state::Pool;

#[derive(Accounts)]
#[instruction(params: LenderClaimParams)]
pub struct LenderClaimAccounts<'info> {
    #[account()]
    pub user: Signer<'info>,

    #[account(
        mut,
        constraint = user_redeemable.mint == pool.redeemable_mint,
        constraint = user_redeemable.owner == user.key(),
        constraint = user_redeemable.delegate == None.into(),
    )]
    pub user_redeemable: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = pool.redeemable_mint == redeemable_mint.key(),
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        associated_token::mint = pool.redeemable_mint,
        associated_token::authority = pool,
    )]
    pub pool_redeemable: Box<Account<'info, TokenAccount>>,

    #[account()]
    pub redeemable_mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        seeds = [
            Lender::SEEDS_PREFIX,
            pool.key().as_ref(),
            user.key().as_ref()
        ],
        bump = lender.bump
    )]
    pub lender: Box<Account<'info, Lender>>,

    #[account()]
    pub token_program: Program<'info, Token>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct LenderClaimParams {
    pub redeemable_amount: u64,
}

pub fn lender_claim_processor(
    context: Context<LenderClaimAccounts>,
    params: LenderClaimParams,
) -> Result<()> {
    let lender = &mut context.accounts.lender;
    let pool = &mut context.accounts.pool;

    if pool.freeze {
        return err!(ProgramError::PoolFreezeIsTrue);
    }
    if !pool.claiming_enabled {
        return err!(ProgramError::PoolClaimingEnabledIsFalse);
    }
    if pool.total_deposited_collateral_amount == 0 {
        return err!(ProgramError::PoolTotalDepositedCollateralAmountIsZero);
    }

    let total_repayed_redeemable_amount = pool.total_claimed_redeemable_amount
        + context.accounts.pool_redeemable.amount;

    // FIX: remove unwrap() to prevent panic / hard DoS
    let claimable_redeemable_amount = u64::try_from(
        u128::from(total_repayed_redeemable_amount)
            .checked_mul(u128::from(lender.deposited_collateral_amount))
            .ok_or(ProgramError::MathOverflow)?
            .checked_div(u128::from(pool.total_deposited_collateral_amount))
            .ok_or(ProgramError::MathOverflow)?,
    )
    .map_err(|_| ProgramError::MathOverflow)?;

    if lender.claimed_redeemable_amount + params.redeemable_amount
        > claimable_redeemable_amount
    {
        return err!(ProgramError::ParamsRedeemableAmountIsTooLarge);
    }

    lender.claimed_redeemable_amount += params.redeemable_amount;
    pool.total_claimed_redeemable_amount += params.redeemable_amount;

    let pool_signer_seeds: &[&[&[u8]]] =
        &[&[Pool::SEEDS_PREFIX, &pool.index.to_le_bytes(), &[pool.bump]]];

    transfer(
        CpiContext::new(
            context.accounts.token_program.to_account_info(),
            Transfer {
                authority: context.accounts.pool.to_account_info(),
                from: context.accounts.pool_redeemable.to_account_info(),
                to: context.accounts.user_redeemable.to_account_info(),
            },
        )
        .with_signer(pool_signer_seeds),
        params.redeemable_amount,
    )?;

    Ok(())
}
