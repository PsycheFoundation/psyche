use anchor_lang::prelude::*;

use crate::ProgramError;
use crate::state::Airdrop;
use crate::state::AirdropMetadata;
use crate::state::MerkleHash;

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

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AirdropUpdateParams {
    pub claim_freeze: Option<bool>,
    pub merkle_root: Option<MerkleHash>,
    pub metadata: Option<AirdropMetadata>,
}

pub fn airdrop_update_processor(
    context: Context<AirdropUpdateAccounts>,
    params: AirdropUpdateParams,
) -> Result<()> {
    let airdrop = &mut context.accounts.airdrop;

    if let Some(claim_freeze) = params.claim_freeze {
        msg!("claim_freeze: {}", claim_freeze);
        airdrop.claim_freeze = claim_freeze;
    }

    if let Some(merkle_root) = params.merkle_root {
        msg!("merkle_root: {:?}", merkle_root);
        if merkle_root == MerkleHash::default() {
            return err!(ProgramError::ParamsMerkleRootIsZeroed);
        }
        airdrop.merkle_root = merkle_root;
    }

    if let Some(metadata) = params.metadata {
        msg!("metadata: {:?}", metadata);
        airdrop.metadata = metadata;
    }

    Ok(())
}
