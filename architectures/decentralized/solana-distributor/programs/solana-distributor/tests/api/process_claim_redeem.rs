use std::collections::HashMap;

use anchor_spl::associated_token;
use anchor_spl::token;
use anyhow::Result;
use psyche_solana_distributor::accounts::ClaimRedeemAccounts;
use psyche_solana_distributor::find_airdrop;
use psyche_solana_distributor::find_claim;
use psyche_solana_distributor::instruction::ClaimRedeem;
use psyche_solana_distributor::logic::ClaimRedeemParams;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_anchor::ToolboxAnchor;
use solana_toolbox_endpoint::ToolboxEndpoint;

pub async fn process_claim_redeem(
    endpoint: &mut ToolboxEndpoint,
    payer: &Keypair,
    claimer: &Keypair,
    receiver_collateral: &Pubkey,
    airdrop_index: u64,
    airdrop_vestings: &HashMap<Pubkey, (i64, u32, u64)>,
    collateral_mint: &Pubkey,
    collateral_amount: u64,
) -> Result<()> {
    let airdrop = find_airdrop(airdrop_index);
    let airdrop_collateral = associated_token::get_associated_token_address(
        &airdrop,
        collateral_mint,
    );

    let claim = find_claim(&airdrop, &claimer.pubkey());

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
                collateral_amount,
                vesting_collateral_amount: 42,
                vesting_duration_seconds: 3600,
                vesting_start_unix_timestamp: 1_700_000_000,
                merkle_proof: vec![],
            },
        },
        payer,
        &[claimer],
    )
    .await?;

    Ok(())
}
