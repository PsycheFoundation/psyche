use anchor_spl::associated_token;
use anchor_spl::token;
use anyhow::Result;
use psyche_solana_distributor::accounts::ClaimRedeemAccounts;
use psyche_solana_distributor::instruction::ClaimRedeem;
use psyche_solana_distributor::logic::ClaimRedeemParams;
use psyche_solana_distributor::state::MerkleHash;
use psyche_solana_distributor::state::Vesting;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

use crate::api::find_pdas::find_pda_airdrop;
use crate::api::find_pdas::find_pda_claim;

#[allow(clippy::too_many_arguments)]
pub async fn process_claim_redeem(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    claimer: &Keypair,
    receiver_collateral: &Pubkey,
    airdrop_index: u64,
    vesting_and_proof: &(Vesting, Vec<MerkleHash>),
    collateral_mint: &Pubkey,
    collateral_amount: u64,
) -> Result<()> {
    let airdrop = find_pda_airdrop(airdrop_index);
    let airdrop_collateral = associated_token::get_associated_token_address(
        &airdrop,
        collateral_mint,
    );

    let claim = find_pda_claim(&airdrop, &claimer.pubkey());

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
                vesting: vesting_and_proof.0,
                merkle_proof: vesting_and_proof.1.to_vec(),
                collateral_amount,
            },
        },
        payer,
        &[claimer],
    )
    .await?;

    Ok(())
}
