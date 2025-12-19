pub mod logic;
pub mod state;

use anchor_lang::prelude::*;
use logic::*;

declare_id!("EnU7DRx5az5YWxaxgqEGbXSYtudcfnjXewyBRRZCjJPw");

pub fn find_run(index: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[state::Run::SEEDS_PREFIX, index.to_le_bytes().as_ref()],
        &crate::ID,
    )
    .0
}

pub fn find_participant(run: &Pubkey, user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            state::Participant::SEEDS_PREFIX,
            run.as_ref(),
            user.as_ref(),
        ],
        &crate::ID,
    )
    .0
}

#[program]
pub mod psyche_solana_treasurer {
    use super::*;

    pub fn run_create(
        context: Context<RunCreateAccounts>,
        params: RunCreateParams,
    ) -> Result<()> {
        run_create_processor(context, params)
    }

    pub fn run_update(
        context: Context<RunUpdateAccounts>,
        params: RunUpdateParams,
    ) -> Result<()> {
        run_update_processor(context, params)
    }

    pub fn participant_create(
        context: Context<ParticipantCreateAccounts>,
        params: ParticipantCreateParams,
    ) -> Result<()> {
        participant_create_processor(context, params)
    }

    pub fn participant_claim(
        context: Context<ParticipantClaimAccounts>,
        params: ParticipantClaimParams,
    ) -> Result<()> {
        participant_claim_processor(context, params)
    }
}

#[error_code]
pub enum ProgramError {
    #[msg("Invalid parameter")]
    InvalidParameter,

    #[msg("run_id must be 32 bytes or less")]
    RunIdInvalidLength,
}
