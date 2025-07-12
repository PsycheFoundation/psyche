use anchor_lang::prelude::*;
use anchor_spl::token::transfer;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;
use anchor_spl::token::Transfer;

use crate::state::Lender;
use crate::state::Pool;
use crate::ProgramError;

#[derive(Accounts)]
#[instruction(params: LenderDepositParams)]
pub struct LenderDepositAccounts<'info> {
    #[account()]
    pub user: Signer<'info>,

    #[account(
        mut,
        constraint = user_collateral.mint == pool.collateral_mint,
        constraint = user_collateral.owner == user.key(),
        constraint = user_collateral.delegate == None.into(),
    )]
    pub user_collateral: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        associated_token::mint = pool.collateral_mint,
        associated_token::authority = pool,
    )]
    pub pool_collateral: Box<Account<'info, TokenAccount>>,

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
pub struct LenderDepositParams {
    pub collateral_amount: u64,
}

pub fn lender_deposit_processor(
    context: Context<LenderDepositAccounts>,
    params: LenderDepositParams,
) -> Result<()> {
    let lender = &mut context.accounts.lender;
    let pool = &mut context.accounts.pool;

    if pool.freeze {
        return err!(ProgramError::PoolFreezeIsTrue);
    }
    if pool.total_deposited_collateral_amount + params.collateral_amount
        > pool.max_deposit_collateral_amount
    {
        return err!(ProgramError::ParamsCollateralAmountIsTooLarge);
    }

    transfer(
        CpiContext::new(
            context.accounts.token_program.to_account_info(),
            Transfer {
                authority: context.accounts.user.to_account_info(),
                from: context.accounts.user_collateral.to_account_info(),
                to: context.accounts.pool_collateral.to_account_info(),
            },
        ),
        params.collateral_amount,
    )?;

    lender.deposited_collateral_amount += params.collateral_amount;
    pool.total_deposited_collateral_amount += params.collateral_amount;

    Ok(())
}
