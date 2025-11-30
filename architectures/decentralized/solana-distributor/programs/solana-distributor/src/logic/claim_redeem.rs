use anchor_lang::prelude::*;
use anchor_spl::token::transfer;
use anchor_spl::token::Mint;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;
use anchor_spl::token::Transfer;

use crate::state::Airdrop;
use crate::state::Claim;
use crate::ProgramError;

#[derive(Accounts)]
#[instruction(params: ClaimRedeemParams)]
pub struct ClaimRedeemAccounts<'info> {
    #[account()]
    pub claimer: Signer<'info>,

    #[account(
        mut,
        constraint = receiver_collateral.mint == airdrop.collateral_mint,
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
            claimer.key().as_ref()
        ],
        bump = claim.bump
    )]
    pub claim: Box<Account<'info, Claim>>,

    #[account()]
    pub token_program: Program<'info, Token>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct ClaimRedeemParams {
    pub vesting_start_unix_timestamp: i64,
    pub vesting_duration_seconds: u64,
    pub vesting_collateral_amount: u64,
    pub collateral_amount: u64,
    pub merkle_proof: Vec<AirdropMerkleHash>,
}

pub fn claim_redeem_processor(
    context: Context<ClaimRedeemAccounts>,
    params: ClaimRedeemParams,
) -> Result<()> {
    let claim = &mut context.accounts.claim;
    let airdrop = &mut context.accounts.airdrop;

    if airdrop.freeze {
        return err!(ProgramError::AirdropFreezeIsTrue);
    }

    let merkle_leaf = hashv(&[
        &context.accounts.claimer.key.to_bytes(),
        &params.vesting_start_unix_timestamp.to_le_bytes(),
        &params.vesting_duration_seconds.to_le_bytes(),
        &params.vesting_collateral_amount.to_le_bytes(),
    ])
    .to_bytes();

    if !merkle_verify(&airdrop.merkle_root, &merkle_leaf, &params.merkle_proof)
    {
        return err!(ProgramError::ParamsMerkleProofIsInvalid);
    }

    let vesting_elapsed_seconds = u128::from(
        Clock::get()?
            .unix_timestamp
            .saturating_sub(params.vesting_start_unix_timestamp),
    );

    let vesting_unlocked_collateral_amount =
        u128::from(params.vesting_collateral_amount)
            .checked_mul(vesting_elapsed_seconds)
            .ok_or(ProgramError::MathOverflow)?
            .checked_div(u128::from(params.vesting_duration_seconds))
            .ok_or(ProgramError::MathOverflow)?
            .saturating_sub(u128::from(claim.claimed_collateral_amount));

    if vesting_unlocked_collateral_amount < u128::from(params.collateral_amount)
    {
        return err!(ProgramError::ParamsCollateralAmountIsTooLarge);
    }

    claim.claimed_collateral_amount += params.collateral_amount;
    airdrop.total_claimed_collateral_amount += params.collateral_amount;

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

fn merkle_verify(
    merkle_root: &AirdropMerkleHash,
    merkle_leaf: &AirdropMerkleHash,
    merkle_proof: &Vec<AirdropMerkleHash>,
) -> bool {
    merkle_proof
        .iter()
        .fold(merkle_leaf, |merkle_hash, merkle_node| {
            if merkle_hash <= *merkle_node {
                hashv(&[&merkle_hash, merkle_node]).to_bytes()
            } else {
                hashv(&[merkle_node, &merkle_hash]).to_bytes()
            }
        })
        == merkle_root
}
