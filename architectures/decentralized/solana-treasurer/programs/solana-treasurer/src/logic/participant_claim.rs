use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token::TokenAccount;
use anchor_spl::token::Transfer;
use anchor_spl::token::transfer;
use psyche_solana_coordinator::CoordinatorAccount;

use crate::ProgramError;
use crate::state::Participant;
use crate::state::Run;

#[derive(Accounts)]
#[instruction(params: ParticipantClaimParams)]
pub struct ParticipantClaimAccounts<'info> {
    #[account()]
    pub user: Signer<'info>,

    #[account(
        mut,
        constraint = user_collateral.mint == run.collateral_mint,
        constraint = user_collateral.owner == user.key(),
        constraint = user_collateral.delegate == None.into(),
    )]
    pub user_collateral: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        constraint = run.coordinator_account == coordinator_account.key(),
    )]
    pub run: Box<Account<'info, Run>>,

    #[account(
        mut,
        associated_token::mint = run.collateral_mint,
        associated_token::authority = run,
    )]
    pub run_collateral: Box<Account<'info, TokenAccount>>,

    #[account(
        constraint = coordinator_account.load()?.version == CoordinatorAccount::VERSION,
    )]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,

    #[account(
        mut,
        seeds = [
            Participant::SEEDS_PREFIX,
            run.key().as_ref(),
            user.key().as_ref()
        ],
        bump = participant.bump
    )]
    pub participant: Box<Account<'info, Participant>>,

    #[account()]
    pub token_program: Program<'info, Token>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ParticipantClaimParams {
    pub claim_earned_points: u64,
}

pub fn participant_claim_processor(
    context: Context<ParticipantClaimAccounts>,
    params: ParticipantClaimParams,
) -> Result<()> {
    let mut participant_earned_points = 0;
    for client in context
        .accounts
        .coordinator_account
        .load()?
        .state
        .clients_state
        .clients
        .iter()
    {
        if client.id.signer == context.accounts.user.key() {
            participant_earned_points = client.earned;
            break;
        }
    }

    let participant = &mut context.accounts.participant;
    let run = &mut context.accounts.run;

    let participant_unclaimed_earned_points =
        participant_earned_points - participant.claimed_earned_points;
    if params.claim_earned_points > participant_unclaimed_earned_points {
        return err!(ProgramError::InvalidParameter);
    }

    // We distribute 1 collateral per point and let the coordinator decide the point reward rate
    let claim_collateral_amount = params.claim_earned_points;

    participant.claimed_collateral_amount += claim_collateral_amount;
    participant.claimed_earned_points += params.claim_earned_points;

    run.total_claimed_collateral_amount += claim_collateral_amount;
    run.total_claimed_earned_points += params.claim_earned_points;

    let run_signer_seeds: &[&[&[u8]]] =
        &[&[Run::SEEDS_PREFIX, &run.index.to_le_bytes(), &[run.bump]]];
    transfer(
        CpiContext::new(
            context.accounts.token_program.to_account_info(),
            Transfer {
                from: context.accounts.run_collateral.to_account_info(),
                to: context.accounts.user_collateral.to_account_info(),
                authority: context.accounts.run.to_account_info(),
            },
        )
        .with_signer(run_signer_seeds),
        claim_collateral_amount,
    )?;

    Ok(())
}
