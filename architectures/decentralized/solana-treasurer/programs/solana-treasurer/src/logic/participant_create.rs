use anchor_lang::prelude::*;

use crate::state::Participant;
use crate::state::Run;

#[derive(Accounts)]
#[instruction(params: ParticipantCreateParams)]
pub struct ParticipantCreateAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub run: Box<Account<'info, Run>>,

    #[account(
        init,
        payer = payer,
        space = Participant::space_with_discriminator(),
        seeds = [
            Participant::SEEDS_PREFIX,
            run.key().as_ref(),
            params.user.as_ref()
        ],
        bump
    )]
    pub participant: Box<Account<'info, Participant>>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ParticipantCreateParams {
    pub user: Pubkey,
}

pub fn participant_create_processor(
    context: Context<ParticipantCreateAccounts>,
    _params: ParticipantCreateParams,
) -> Result<()> {
    let participant = &mut context.accounts.participant;
    participant.bump = context.bumps.participant;

    participant.claimed_earned_points = 0;
    participant.claimed_collateral_amount = 0;

    Ok(())
}
