use anchor_lang::prelude::*;

use crate::bytes_from_string;
use crate::program_error::ProgramError;
use crate::CoordinatorAccount;
use crate::CoordinatorInstance;

#[derive(Accounts)]
#[instruction(params: FreeCoordinatorParams)]
pub struct FreeCoordinatorAccounts<'info> {
    #[account()]
    pub authority: Signer<'info>,

    /// CHECK: TODO why is this ok to be unchecked?
    #[account(mut)]
    pub spill: SystemAccount<'info>,

    #[account(
        mut,
        seeds = [
            CoordinatorInstance::SEEDS_PREFIX,
            bytes_from_string(&coordinator_instance.run_id)
        ],
        bump = coordinator_instance.bump,
        constraint = coordinator_instance.main_authority == authority.key(),
        close = spill,
    )]
    pub coordinator_instance: Box<Account<'info, CoordinatorInstance>>,

    #[account(
        mut,
        constraint = coordinator_instance.coordinator_account == coordinator_account.key(),
        close = spill,
    )]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct FreeCoordinatorParams {}

pub fn free_coordinator_processor(
    context: Context<FreeCoordinatorAccounts>,
    _params: FreeCoordinatorParams,
) -> Result<()> {
    if !&context
        .accounts
        .coordinator_account
        .load()?
        .state
        .coordinator
        .halted()
    {
        return err!(ProgramError::CloseCoordinatorNotHalted);
    }
    Ok(())
}
