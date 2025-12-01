use anchor_spl::associated_token;
use anchor_spl::token;
use anyhow::Result;
use psyche_solana_distributor::accounts::AirdropCreateAccounts;
use psyche_solana_distributor::instruction::AirdropCreate;
use psyche_solana_distributor::logic::AirdropCreateParams;
use psyche_solana_distributor::state::AirdropMetadata;
use psyche_solana_distributor::state::MerkleHash;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::system_program;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

use crate::api::find_pdas::find_pda_airdrop;

pub async fn process_airdrop_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    airdrop_id: u64,
    airdrop_authority: &Keypair,
    airdrop_merkle_root: MerkleHash,
    airdrop_metadata: AirdropMetadata,
    collateral_mint: &Pubkey,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    let airdrop_collateral = associated_token::get_associated_token_address(
        &airdrop,
        collateral_mint,
    );

    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_distributor::id(),
        AirdropCreateAccounts {
            payer: payer.pubkey(),
            authority: airdrop_authority.pubkey(),
            airdrop,
            airdrop_collateral,
            collateral_mint: *collateral_mint,
            associated_token_program: associated_token::ID,
            token_program: token::ID,
            system_program: system_program::ID,
        },
        AirdropCreate {
            params: AirdropCreateParams {
                id: airdrop_id,
                merkle_root: airdrop_merkle_root,
                metadata: airdrop_metadata,
            },
        },
        payer,
        &[airdrop_authority],
    )
    .await?;

    Ok(())
}
