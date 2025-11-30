use psyche_solana_distributor::state::{Airdrop, AirdropMetadata, Claim};
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

use crate::api::create_memnet_endpoint::create_memnet_endpoint;
use crate::api::process_airdrop_create::process_airdrop_create;

#[tokio::test]
pub async fn run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Test constants
    let payer = Keypair::new();
    let payer_lamports = 1_000_000_000;

    let airdrop_index = 42u64;
    let airdrop_authority = Keypair::new();
    let airdrop_authority_collateral_amount = 424242;

    let collateral_mint_authority = Keypair::new();
    let collateral_mint_decimals = 6;

    let user = Keypair::new();
    let receiver = Keypair::new();

    // Prepare the payer
    endpoint
        .request_airdrop(&payer.pubkey(), payer_lamports)
        .await
        .unwrap();

    // Create the global collateral mint
    let collateral_mint = endpoint
        .process_spl_token_mint_new(
            &payer,
            &collateral_mint_authority.pubkey(),
            None,
            collateral_mint_decimals,
        )
        .await
        .unwrap();

    // Create collateral ATAs
    let receiver_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &receiver.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();
    let airdrop_authority_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &airdrop_authority.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();

    // Give the airdrop_authority some collateral
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &collateral_mint,
            &collateral_mint_authority,
            &airdrop_authority_collateral,
            airdrop_authority_collateral_amount,
        )
        .await
        .unwrap();

    // Create the funding airdrop
    process_airdrop_create(
        &mut endpoint,
        &payer,
        airdrop_index,
        &airdrop_authority,
        [0u8; 32],
        AirdropMetadata {
            length: 0,
            bytes: [0u8; AirdropMetadata::BYTES],
        },
        &collateral_mint,
    )
    .await
    .unwrap();
}
