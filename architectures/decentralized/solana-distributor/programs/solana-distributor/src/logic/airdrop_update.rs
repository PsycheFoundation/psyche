use anchor_lang::prelude::*;

use crate::state::Airdrop;
use crate::state::AirdropMetadata;
use crate::ProgramError;

#[derive(Accounts)]
#[instruction(params: AirdropUpdateParams)]
pub struct AirdropUpdateAccounts<'info> {
    #[account()]
    pub authority: Signer<'info>,

    #[account(
        mut,
        constraint = airdrop.authority == authority.key(),
    )]
    pub airdrop: Box<Account<'info, Airdrop>>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct AirdropUpdateParams {
    pub freeze: Option<bool>,
    pub merkle_root: Option<AirdropMerkleHash>,
    pub metadata: Option<AirdropMetadata>,
}

pub fn airdrop_update_processor(
    context: Context<AirdropUpdateAccounts>,
    params: AirdropUpdateParams,
) -> Result<()> {
    let airdrop = &mut context.accounts.airdrop;

    if let Some(freeze) = params.freeze {
        msg!("freeze: {}", freeze);
        airdrop.freeze = freeze;
    }

    if let Some(merkle_root) = params.merkle_root {
        msg!("merkle_root: {:?}", merkle_root);
        airdrop.merkle_root = merkle_root;
    }

    if let Some(metadata) = params.metadata {
        if usize::from(metadata.length) > AirdropMetadata::BYTES {
            return err!(ProgramError::ParamsMetadataLengthIsTooLarge);
        }
        airdrop.metadata = metadata;
    }

    Ok(())
}
