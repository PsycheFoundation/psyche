use psyche_solana_distributor::state::AirdropMetadata;
use psyche_solana_distributor::state::Allocation;
use psyche_solana_distributor::state::Vesting;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

use crate::api::airdrop_data::AirdropMerkleTree;
use crate::api::create_memnet_endpoint::create_memnet_endpoint;
use crate::api::find_pdas::find_pda_airdrop;
use crate::api::process_airdrop_create::process_airdrop_create;
use crate::api::process_claim_create::process_claim_create;
use crate::api::process_claim_redeem::process_claim_redeem;

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

    let claimer = Keypair::new();
    let claimer_vesting = Vesting {
        start_unix_timestamp: 100,
        duration_seconds: 100,
        end_collateral_amount: 323232,
    };

    let receiver = Keypair::new();

    let merkle_tree = AirdropMerkleTree::try_from(&vec![Allocation {
        receiver: claimer.pubkey(),
        vesting: claimer_vesting,
    }])
    .unwrap();

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
        *merkle_tree.root().unwrap(),
        AirdropMetadata {
            length: 0,
            bytes: [0u8; AirdropMetadata::BYTES],
        },
        &collateral_mint,
    )
    .await
    .unwrap();

    // Create the claim PDA
    process_claim_create(&mut endpoint, &payer, &claimer, airdrop_index)
        .await
        .unwrap();

    // Redeem nothing should work with a valid proof
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_index,
        &claimer_vesting,
        &merkle_tree.proofs_for_receiver(&claimer.pubkey()).unwrap()[0].1,
        &collateral_mint,
        0,
    )
    .await
    .unwrap();

    // Redeem again should fail (not enough collateral in airdrop)
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_index,
        &claimer_vesting,
        &merkle_tree.proofs_for_receiver(&claimer.pubkey()).unwrap()[0].1,
        &collateral_mint,
        1,
    )
    .await
    .unwrap_err();

    // Give the airdrop enough collateral
    let airdrop = find_pda_airdrop(airdrop_index);
    let airdrop_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &airdrop,
            &collateral_mint,
        )
        .await
        .unwrap();
    endpoint
        .process_spl_token_transfer(
            &payer,
            &airdrop_authority,
            &airdrop_authority_collateral,
            &airdrop_collateral,
            airdrop_authority_collateral_amount,
        )
        .await
        .unwrap();

    // Redeem full amount should work now
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_index,
        &claimer_vesting,
        &merkle_tree.proofs_for_receiver(&claimer.pubkey()).unwrap()[0].1,
        &collateral_mint,
        claimer_vesting.end_collateral_amount,
    )
    .await
    .unwrap();

    // Redeem anything more than that should fail
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_index,
        &claimer_vesting,
        &merkle_tree.proofs_for_receiver(&claimer.pubkey()).unwrap()[0].1,
        &collateral_mint,
        1,
    )
    .await
    .unwrap_err();
}
