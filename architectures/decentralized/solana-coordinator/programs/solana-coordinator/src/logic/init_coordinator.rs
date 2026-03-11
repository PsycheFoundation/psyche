use anchor_lang::prelude::*;
use psyche_coordinator::SOLANA_RUN_ID_MAX_LEN;
use psyche_core::FixedString;

use crate::CoordinatorAccount;
use crate::CoordinatorInstance;
use crate::ProgramError;
use crate::bytes_from_string;

#[derive(Accounts)]
#[instruction(params: InitCoordinatorParams)]
pub struct InitCoordinatorAccounts<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account()]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + CoordinatorInstance::INIT_SPACE,
        seeds = [
            CoordinatorInstance::SEEDS_PREFIX,
            bytes_from_string(&params.run_id)
        ],
        bump
    )]
    pub coordinator_instance: Box<Account<'info, CoordinatorInstance>>,

    /// CHECK: Account will be completely re-written in this instruction and not read
    #[account(
        mut,
        owner = crate::ID,
    )]
    pub coordinator_account: UncheckedAccount<'info>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitCoordinatorParams {
    pub run_id: String,
    pub join_authority: Pubkey,
    pub client_version: String,
}

pub fn init_coordinator_processor(
    context: Context<InitCoordinatorAccounts>,
    params: InitCoordinatorParams,
) -> Result<()> {
    if params.run_id.is_empty() {
        return err!(ProgramError::RunIdInvalidLength);
    }
    if params.run_id.len() > SOLANA_RUN_ID_MAX_LEN {
        return err!(ProgramError::RunIdInvalidLength);
    }

    // Initialize the coordinator instance
    let coordinator_instance = &mut context.accounts.coordinator_instance;
    coordinator_instance.bump = context.bumps.coordinator_instance;
    coordinator_instance.main_authority = context.accounts.authority.key();
    coordinator_instance.join_authority = params.join_authority;
    coordinator_instance.coordinator_account =
        context.accounts.coordinator_account.key();
    coordinator_instance.run_id = params.run_id.clone();

    // Initialize the coordinator account
    let mut data =
        context.accounts.coordinator_account.try_borrow_mut_data()?;
    if data.len() != CoordinatorAccount::space_with_discriminator() {
        return err!(ProgramError::CoordinatorAccountIncorrectSize);
    }

    // Install the correct coordinator account's discriminator, verify that it was zero before init
    let disc = CoordinatorAccount::DISCRIMINATOR;
    let data_disc = &mut data[..disc.len()];
    if data_disc.iter().any(|b| *b != 0) {
        return err!(ErrorCode::AccountDiscriminatorAlreadySet);
    }
    data_disc.copy_from_slice(disc);

    // Ready to prepare the coordinator content
    let account = bytemuck::from_bytes_mut::<CoordinatorAccount>(
        &mut data[disc.len()..CoordinatorAccount::space_with_discriminator()],
    );
    account.version = CoordinatorAccount::VERSION;
    account.nonce = 0;

    // Setup the run_id const
    account.state.coordinator.run_id =
        FixedString::try_from(params.run_id.as_str())
            .map_err(|_| ProgramError::FixedStringTooLong)?;

    // First client version
    account.state.client_version =
        FixedString::try_from(params.client_version.as_str())
            .map_err(|_| ProgramError::FixedStringTooLong)?;

    // Done
    Ok(())
}
