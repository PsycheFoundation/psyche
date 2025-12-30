use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::Mint;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;

use crate::state::Airdrop;
use crate::state::AirdropMetadata;
use crate::state::MerkleHash;

#[derive(Accounts)]
#[instruction(params: AirdropCreateParams)]
pub struct AirdropCreateAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = Airdrop::space_with_discriminator(),
        seeds = [Airdrop::SEEDS_PREFIX, &params.id.to_le_bytes()],
        bump,
    )]
    pub airdrop: Box<Account<'info, Airdrop>>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = collateral_mint,
        associated_token::authority = airdrop,
    )]
    pub airdrop_collateral: Box<Account<'info, TokenAccount>>,

    #[account()]
    pub collateral_mint: Box<Account<'info, Mint>>,

    #[account()]
    pub associated_token_program: Program<'info, AssociatedToken>,

    #[account()]
    pub token_program: Program<'info, Token>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AirdropCreateParams {
    pub id: u64,
    pub merkle_root: MerkleHash,
    pub metadata: AirdropMetadata,
}

pub fn airdrop_create_processor(
    context: Context<AirdropCreateAccounts>,
    params: AirdropCreateParams,
) -> Result<()> {
    let airdrop = &mut context.accounts.airdrop;

    airdrop.bump = context.bumps.airdrop;

    airdrop.id = params.id;
    airdrop.authority = context.accounts.authority.key();

    airdrop.collateral_mint = context.accounts.collateral_mint.key();
    airdrop.total_claimed_collateral_amount = 0;

    airdrop.claim_freeze = false;
    airdrop.merkle_root = params.merkle_root;
    airdrop.metadata = params.metadata;

    Ok(())
}
