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

use crate::api::distributor_state::find_pda_airdrop;
use crate::api::distributor_state::find_pda_claim;

pub async fn process_airdrop_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    airdrop_id: u64,
    airdrop_authority: &Keypair,
    airdrop_merkle_root: &MerkleHash,
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
                merkle_root: airdrop_merkle_root.clone(),
                metadata: airdrop_metadata,
            },
        },
        payer,
        &[airdrop_authority],
    )
    .await?;
    Ok(())
}

pub async fn process_airdrop_update(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    airdrop_id: u64,
    airdrop_authority: &Keypair,
    airdrop_claim_freeze: Option<bool>,
    airdrop_merkle_root: Option<MerkleHash>,
    airdrop_metadata: Option<AirdropMetadata>,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_distributor::id(),
        AirdropUpdateAccounts {
            authority: airdrop_authority.pubkey(),
            airdrop,
        },
        AirdropUpdate {
            params: AirdropUpdateParams {
                claim_freeze: airdrop_claim_freeze,
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

pub async fn process_airdrop_withdraw(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    airdrop_id: u64,
    airdrop_authority: &Keypair,
    receiver_collateral: &Pubkey,
    collateral_mint: &Pubkey,
    collateral_amount: u64,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    let airdrop_collateral = associated_token::get_associated_token_address(
        &airdrop,
        collateral_mint,
    );
    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_distributor::id(),
        AirdropWithdrawAccounts {
            authority: airdrop_authority.pubkey(),
            receiver_collateral: *receiver_collateral,
            airdrop,
            airdrop_collateral,
            collateral_mint: *collateral_mint,
            associated_token_program: associated_token::ID,
            token_program: token::ID,
            system_program: system_program::ID,
        },
        AirdropWithdraw {
            params: AirdropWithdrawParams { collateral_amount },
        },
        payer,
        &[airdrop_authority],
    )
    .await?;
    Ok(())
}

pub async fn process_claim_create(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    claimer: &Keypair,
    airdrop_id: u64,
    allocation_nonce: u64,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    let claim = find_pda_claim(&airdrop, &claimer.pubkey(), allocation_nonce);
    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_distributor::id(),
        ClaimCreateAccounts {
            payer: payer.pubkey(),
            claimer: claimer.pubkey(),
            airdrop,
            claim,
            system_program: system_program::ID,
        },
        ClaimCreate {
            params: ClaimCreateParams {
                nonce: allocation_nonce,
            },
        },
        payer,
        &[claimer],
    )
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn process_claim_redeem(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    claimer: &Keypair,
    receiver_collateral: &Pubkey,
    airdrop_id: u64,
    allocation_nonce: u64,
    allocation_vesting: &Vesting,
    merkle_proof: &[MerkleHash],
    collateral_mint: &Pubkey,
    collateral_amount: u64,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_id);
    let airdrop_collateral = associated_token::get_associated_token_address(
        &airdrop,
        collateral_mint,
    );
    let claim = find_pda_claim(&airdrop, &claimer.pubkey(), allocation_nonce);
    ToolboxAnchor::process_instruction_with_signers(
        endpoint,
        psyche_solana_distributor::id(),
        ClaimRedeemAccounts {
            claimer: claimer.pubkey(),
            receiver_collateral: *receiver_collateral,
            airdrop,
            airdrop_collateral,
            collateral_mint: *collateral_mint,
            claim,
            token_program: token::ID,
        },
        ClaimRedeem {
            params: ClaimRedeemParams {
                nonce: allocation_nonce,
                vesting: allocation_vesting.clone(),
                merkle_proof: merkle_proof.to_vec(),
                collateral_amount,
            },
        },
        payer,
        &[claimer],
    )
    .await?;
    Ok(())
}
