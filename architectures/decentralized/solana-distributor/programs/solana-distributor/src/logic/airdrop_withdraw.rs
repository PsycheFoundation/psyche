use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::transfer;
use anchor_spl::token::Mint;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;
use anchor_spl::token::Transfer;

use crate::state::Airdrop;
use crate::ProgramError;

#[derive(Accounts)]
#[instruction(params: AirdropWithdrawParams)]
pub struct AirdropWithdrawAccounts<'info> {
    #[account()]
    pub authority: Signer<'info>,

    #[account(
        mut,
        constraint = receiver_collateral.mint == airdrop.collateral_mint,
        constraint = receiver_collateral.delegate == None.into(),
    )]
    pub receiver_collateral: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = airdrop.authority == authority.key(),
    )]
    pub airdrop: Box<Account<'info, Airdrop>>,

    #[account(
        mut,
        associated_token::mint = collateral_mint,
        associated_token::authority = airdrop,
    )]
    pub airdrop_collateral: Box<Account<'info, TokenAccount>>,

    #[account()]
    pub collateral_mint: Box<Account<'info, Mint>>,

    #[account()]
    pub token_program: Program<'info, Token>,

    #[account()]
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct AirdropWithdrawParams {
    pub collateral_amount: u64,
}

pub fn airdrop_withdraw_processor(
    context: Context<AirdropWithdrawAccounts>,
    params: AirdropWithdrawParams,
) -> Result<()> {
    let airdrop = &mut context.accounts.airdrop;
    if airdrop.freeze {
        return err!(ProgramError::AirdropFreezeIsTrue);
    }

    let airdrop_signer_seeds: &[&[&[u8]]] = &[&[
        Airdrop::SEEDS_PREFIX,
        &airdrop.index.to_le_bytes(),
        &[airdrop.bump],
    ]];
    transfer(
        CpiContext::new(
            context.accounts.token_program.to_account_info(),
            Transfer {
                authority: context.accounts.airdrop.to_account_info(),
                from: context.accounts.airdrop_collateral.to_account_info(),
                to: context.accounts.receiver_collateral.to_account_info(),
            },
        )
        .with_signer(airdrop_signer_seeds),
        params.collateral_amount,
    )?;

    Ok(())
}
