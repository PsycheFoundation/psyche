use anchor_lang::prelude::*;
use anchor_spl::token::Mint;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;
use anchor_spl::token::Transfer;
use anchor_spl::token::transfer;

use crate::ProgramError;
use crate::state::Airdrop;
use crate::state::Allocation;
use crate::state::Claim;
use crate::state::MerkleHash;
use crate::state::Vesting;

#[derive(Accounts)]
#[instruction(params: ClaimRedeemParams)]
pub struct ClaimRedeemAccounts<'info> {
    #[account()]
    pub claimer: Signer<'info>,

    #[account(
        mut,
        constraint = receiver_collateral.mint == airdrop.collateral_mint,
        constraint = receiver_collateral.delegate == None.into(),
    )]
    pub receiver_collateral: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = airdrop.collateral_mint == collateral_mint.key(),
    )]
    pub airdrop: Box<Account<'info, Airdrop>>,

    #[account(
        mut,
        associated_token::mint = airdrop.collateral_mint,
        associated_token::authority = airdrop,
    )]
    pub airdrop_collateral: Box<Account<'info, TokenAccount>>,

    #[account()]
    pub collateral_mint: Box<Account<'info, Mint>>,

    #[account(
        mut,
        seeds = [
            Claim::SEEDS_PREFIX,
            airdrop.key().as_ref(),
            claimer.key().as_ref(),
            params.nonce.to_le_bytes().as_ref()
        ],
        bump = claim.bump
    )]
    pub claim: Box<Account<'info, Claim>>,

    #[account()]
    pub token_program: Program<'info, Token>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ClaimRedeemParams {
    pub nonce: u64,
    pub vesting: Vesting,
    pub collateral_amount: u64,
    pub merkle_proof: Vec<MerkleHash>,
}

pub fn claim_redeem_processor(
    context: Context<ClaimRedeemAccounts>,
    params: ClaimRedeemParams,
) -> Result<()> {
    let airdrop = &mut context.accounts.airdrop;
    if airdrop.claim_freeze {
        return err!(ProgramError::AirdropClaimFreezeIsTrue);
    }

    let allocation = Allocation {
        claimer: context.accounts.claimer.key(),
        nonce: params.nonce,
        vesting: params.vesting,
    };
    if !airdrop
        .merkle_root
        .is_valid_proof(&allocation.to_merkle_hash(), &params.merkle_proof)
    {
        return err!(ProgramError::ParamsMerkleProofIsInvalid);
    }

    let vested_collateral_amount = params
        .vesting
        .compute_vested_collateral_amount(Clock::get()?.unix_timestamp)?;

    let claim = &mut context.accounts.claim;
    let claimable_collateral_amount = vested_collateral_amount
        .saturating_sub(claim.claimed_collateral_amount);
    if claimable_collateral_amount < params.collateral_amount {
        return err!(ProgramError::ParamsCollateralAmountIsTooLarge);
    }

    claim.claimed_collateral_amount += params.collateral_amount;
    airdrop.total_claimed_collateral_amount += params.collateral_amount;

    let airdrop_signer_seeds: &[&[&[u8]]] = &[&[
        Airdrop::SEEDS_PREFIX,
        &airdrop.id.to_le_bytes(),
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
