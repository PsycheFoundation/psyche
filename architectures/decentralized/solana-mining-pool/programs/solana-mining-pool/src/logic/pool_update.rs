use anchor_lang::prelude::*;

use crate::state::Pool;
use crate::state::PoolMetadata;
use crate::ProgramError;

#[derive(Accounts)]
#[instruction(params: PoolUpdateParams)]
pub struct PoolUpdateAccounts<'info> {
    #[account()]
    pub authority: Signer<'info>,

    #[account(
        mut,
        constraint = pool.authority == authority.key(),
    )]
    pub pool: Box<Account<'info, Pool>>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct PoolUpdateParams {
    pub max_deposit_collateral_amount: Option<u64>,
    pub freeze: Option<bool>,
    pub metadata: Option<PoolMetadata>,
}

pub fn pool_update_processor(
    context: Context<PoolUpdateAccounts>,
    params: PoolUpdateParams,
) -> Result<()> {
    let pool = &mut context.accounts.pool;

    if let Some(max_deposit_collateral_amount) =
        params.max_deposit_collateral_amount
    {
        msg!(
            "max_deposit_collateral_amount: {}",
            max_deposit_collateral_amount
        );
        pool.max_deposit_collateral_amount = max_deposit_collateral_amount;
    }

    if let Some(freeze) = params.freeze {
        msg!("freeze: {}", freeze);
        pool.freeze = freeze;
    }

    if let Some(metadata) = params.metadata {
        if usize::from(metadata.length) > PoolMetadata::BYTES {
            return err!(ProgramError::ParamsMetadataLengthIsTooLarge);
        }
        pool.metadata = metadata;
    }

    Ok(())
}
