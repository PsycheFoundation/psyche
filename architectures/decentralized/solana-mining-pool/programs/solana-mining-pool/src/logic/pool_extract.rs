use anchor_lang::prelude::*;
use anchor_spl::token::transfer;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;
use anchor_spl::token::Transfer;

use crate::state::Pool;
use crate::ProgramError;

#[derive(Accounts)]
#[instruction(params: PoolExtractParams)]
pub struct PoolExtractAccounts<'info> {
    #[account()]
    pub authority: Signer<'info>,

    #[account(
        mut,
        constraint = authority_collateral.mint == pool.collateral_mint,
        constraint = authority_collateral.owner == authority.key(),
        constraint = authority_collateral.delegate == None.into(),
    )]
    pub authority_collateral: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = pool.authority == authority.key(),
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        associated_token::mint = pool.collateral_mint,
        associated_token::authority = pool,
    )]
    pub pool_collateral: Box<Account<'info, TokenAccount>>,

    #[account()]
    pub token_program: Program<'info, Token>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct PoolExtractParams {
    pub collateral_amount: u64,
}

pub fn pool_extract_processor(
    context: Context<PoolExtractAccounts>,
    params: PoolExtractParams,
) -> Result<()> {
    let pool = &mut context.accounts.pool;

    if pool.freeze {
        return err!(ProgramError::PoolFreezeIsTrue);
    }

    pool.total_extracted_collateral_amount += params.collateral_amount;

    let pool_signer_seeds: &[&[&[u8]]] =
        &[&[Pool::SEEDS_PREFIX, &pool.index.to_le_bytes(), &[pool.bump]]];
    transfer(
        CpiContext::new(
            context.accounts.token_program.to_account_info(),
            Transfer {
                authority: context.accounts.pool.to_account_info(),
                from: context.accounts.pool_collateral.to_account_info(),
                to: context.accounts.authority_collateral.to_account_info(),
            },
        )
        .with_signer(pool_signer_seeds),
        params.collateral_amount,
    )?;

    Ok(())
}
