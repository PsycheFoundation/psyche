pub mod logic;
pub mod state;

use anchor_lang::prelude::*;
use logic::*;

declare_id!("AbPtyECaUE9kYyyu5hiEa5LF1yffU7kwso2AnDJebvr1");

pub fn find_authorization(
    grantor: &Pubkey,
    grantee: &Pubkey,
    scope: &[u8],
) -> Pubkey {
    Pubkey::find_program_address(
        &[
            state::Authorization::SEEDS_PREFIX,
            grantor.as_ref(),
            grantee.as_ref(),
            scope,
        ],
        &crate::ID,
    )
    .0
}

#[program]
pub mod psyche_solana_authorizer {
    use super::*;

    pub fn authorization_create(
        context: Context<AuthorizationCreateAccounts>,
        params: AuthorizationCreateParams,
    ) -> Result<()> {
        authorization_create_processor(context, params)
    }

    pub fn authorization_grantor_update(
        context: Context<AuthorizationGrantorUpdateAccounts>,
        params: AuthorizationGrantorUpdateParams,
    ) -> Result<()> {
        authorization_grantor_update_processor(context, params)
    }

    pub fn authorization_grantee_update(
        context: Context<AuthorizationGranteeUpdateAccounts>,
        params: AuthorizationGranteeUpdateParams,
    ) -> Result<()> {
        authorization_grantee_update_processor(context, params)
    }

    pub fn authorization_close(
        context: Context<AuthorizationCloseAccounts>,
        params: AuthorizationCloseParams,
    ) -> Result<()> {
        authorization_close_processor(context, params)
    }
}

#[error_code]
pub enum ProgramError {
    #[msg("AuthorizationActive is true")]
    AuthorizationActiveIsTrue,
    #[msg("Authorization closing conditions not reached yet")]
    AuthorizationClosingConditionsNotReachedYet,
}
