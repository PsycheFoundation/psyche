use anchor_lang::prelude::*;

use crate::state::Authorization;
use crate::ProgramError;

#[derive(Accounts)]
#[instruction(params: AuthorizationCloseParams)]
pub struct AuthorizationCloseAccounts<'info> {
    #[account()]
    pub grantor: Signer<'info>,

    #[account(mut)]
    pub spill: SystemAccount<'info>,

    #[account(
        mut,
        constraint = authorization.grantor == grantor.key(),
        close = spill,
    )]
    pub authorization: Box<Account<'info, Authorization>>,

    #[account()]
    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AuthorizationCloseParams {}

pub fn authorization_close_processor(
    context: Context<AuthorizationCloseAccounts>,
    _params: AuthorizationCloseParams,
) -> Result<()> {
    let authorization = &context.accounts.authorization;

    if authorization.active {
        return err!(ProgramError::AuthorizationActiveIsTrue);
    }

    if !authorization.delegates.is_empty()
        && Clock::get()?.unix_timestamp
            < authorization
                .grantor_update_unix_timestamp
                .saturating_add(30 * 24 * 60 * 60)
    {
        return err!(ProgramError::AuthorizationClosingConditionsNotReachedYet);
    }

    Ok(())
}
