use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Mint;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;

use crate::state::Pool;
use crate::state::PoolMetadata;
use crate::ProgramError;

#[derive(Accounts)]
#[instruction(params: PoolCreateParams)]
pub struct PoolCreateAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = Pool::space_with_discriminator(),
        seeds = [Pool::SEEDS_PREFIX, &params.index.to_le_bytes()],
        bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = collateral_mint,
        associated_token::authority = pool,
    )]
    pub pool_collateral: Box<Account<'info, TokenAccount>>,

    #[account()]
    pub collateral_mint: Box<Account<'info, Mint>>,

    #[account()]
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account()]
    pub token_program: Program<'info, Token>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct PoolCreateParams {
    pub index: u64,
    pub metadata: PoolMetadata,
}

pub fn pool_create_processor(
    context: Context<PoolCreateAccounts>,
    params: PoolCreateParams,
) -> Result<()> {
    if usize::from(params.metadata.length) > PoolMetadata::BYTES {
        return err!(ProgramError::ParamsMetadataLengthIsTooLarge);
    }

    let pool = &mut context.accounts.pool;

    pool.bump = context.bumps.pool;

    pool.index = params.index;
    pool.authority = context.accounts.authority.key();

    pool.collateral_mint = context.accounts.collateral_mint.key();
    pool.max_deposit_collateral_amount = 0;
    pool.total_deposited_collateral_amount = 0;
    pool.total_extracted_collateral_amount = 0;

    pool.claiming_enabled = false;
    pool.redeemable_mint = Pubkey::default();
    pool.total_claimed_redeemable_amount = 0;

    pool.freeze = false;
    pool.metadata = params.metadata;

    Ok(())
}
