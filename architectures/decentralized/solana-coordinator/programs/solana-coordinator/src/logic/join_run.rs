use anchor_lang::prelude::*;
use psyche_solana_authorizer::state::Authorization;

use crate::ClientId;
use crate::CoordinatorAccount;
use crate::CoordinatorInstance;
use crate::bytes_from_string;
use crate::program_error::ProgramError;

pub const JOIN_RUN_AUTHORIZATION_SCOPE: &[u8] = b"CoordinatorJoinRun";

#[derive(Accounts)]
#[instruction(params: JoinRunParams)]
pub struct JoinRunAccounts<'info> {
    #[account()]
    pub user: Signer<'info>,

    #[account(
        constraint = authorization.is_valid_for(
            &coordinator_instance.join_authority,
            user.key,
            JOIN_RUN_AUTHORIZATION_SCOPE,
        ),
    )]
    pub authorization: Box<Account<'info, Authorization>>,

    #[account(
        seeds = [
            CoordinatorInstance::SEEDS_PREFIX,
            bytes_from_string(&coordinator_instance.run_id)
        ],
        bump = coordinator_instance.bump,
    )]
    pub coordinator_instance: Box<Account<'info, CoordinatorInstance>>,

    #[account(
        mut,
        constraint = coordinator_instance.coordinator_account == coordinator_account.key(),
        constraint = coordinator_account.load()?.version == CoordinatorAccount::VERSION,
    )]
    pub coordinator_account: AccountLoader<'info, CoordinatorAccount>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct JoinRunParams {
    pub client_id: ClientId,
    pub claimer: Pubkey,
}

pub fn join_run_processor(
    context: Context<JoinRunAccounts>,
    params: JoinRunParams,
) -> Result<()> {
    if &params.client_id.signer != context.accounts.user.key {
        return err!(ProgramError::SignerMismatch);
    }
    let mut account = context.accounts.coordinator_account.load_mut()?;
    account.increment_nonce();
    account.state.join_run(params.client_id, params.claimer)
}
