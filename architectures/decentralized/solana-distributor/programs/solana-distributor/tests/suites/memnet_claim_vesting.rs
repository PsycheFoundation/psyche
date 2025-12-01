use anchor_spl::associated_token;
use psyche_solana_distributor::state::AirdropMetadata;
use psyche_solana_distributor::state::Allocation;
use psyche_solana_distributor::state::Vesting;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

use crate::api::airdrop_merkle_tree::AirdropMerkleTree;
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

    let airdrop_id = 42u64;
    let airdrop_authority = Keypair::new();
    let airdrop_collateral_amount = 999_999_999 * 1_000_000;

    let collateral_mint_authority = Keypair::new();
    let collateral_mint_decimals = 6;

    let now_unix_timestamp =
        endpoint.get_sysvar_clock().await.unwrap().unix_timestamp;
    let vesting_start_delay_seconds = 1000u32;
    let vesting_duration_seconds = 1_000_000;
    let vesting_per_second_collateral_amount = 1_000_000;

    // Airdrop merkle tree content
    let claimer = Keypair::new();
    let claimer_vesting = Vesting {
        start_unix_timestamp: now_unix_timestamp
            + i64::from(vesting_start_delay_seconds),
        duration_seconds: vesting_duration_seconds,
        end_collateral_amount: u64::from(vesting_duration_seconds)
            * vesting_per_second_collateral_amount,
    };
    let airdrop_merkle_tree = AirdropMerkleTree::try_from(&vec![Allocation {
        claimer: claimer.pubkey(),
        nonce: 77,
        vesting: claimer_vesting,
    }])
    .unwrap();

    // Prepare the payer
    endpoint
        .request_airdrop(&payer.pubkey(), payer_lamports)
        .await
        .unwrap();

    // Create the collateral_mint
    let collateral_mint = endpoint
        .process_spl_token_mint_new(
            &payer,
            &collateral_mint_authority.pubkey(),
            None,
            collateral_mint_decimals,
        )
        .await
        .unwrap();

    // Create the airdrop
    process_airdrop_create(
        &mut endpoint,
        &payer,
        airdrop_id,
        &airdrop_authority,
        *airdrop_merkle_tree.root().unwrap(),
        AirdropMetadata {
            length: 0,
            bytes: [0u8; AirdropMetadata::BYTES],
        },
        &collateral_mint,
    )
    .await
    .unwrap();

    // Fill up the airdrop's collateral vault
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &collateral_mint,
            &collateral_mint_authority,
            &associated_token::get_associated_token_address(
                &find_pda_airdrop(airdrop_id),
                &collateral_mint,
            ),
            airdrop_collateral_amount,
        )
        .await
        .unwrap();

    // Get the claimer's allocation and proof
    let claimer_allocation = airdrop_merkle_tree.allocations()[0];
    let claimer_merkle_proof =
        airdrop_merkle_tree.proof_at_allocation_index(0).unwrap();

    // Create the claim PDA
    process_claim_create(
        &mut endpoint,
        &payer,
        &claimer,
        airdrop_id,
        claimer_allocation.nonce,
    )
    .await
    .unwrap();

    // Create a wallet that will receive the airdrop's claimed collateral
    let receiver_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &Pubkey::new_unique(),
            &collateral_mint,
        )
        .await
        .unwrap();

    // Redeem nothing should work with a valid input
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        &claimer_allocation,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap();

    // Redeeming before vesting start should fail
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        &claimer_allocation,
        &claimer_merkle_proof,
        &collateral_mint,
        1,
    )
    .await
    .unwrap_err();

    // Move time forward to exactly vesting start
    endpoint
        .forward_clock_unix_timestamp(u64::from(vesting_start_delay_seconds))
        .await
        .unwrap();

    // Redeeming right at the start of vesting should still fail
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        &claimer_allocation,
        &claimer_merkle_proof,
        &collateral_mint,
        1,
    )
    .await
    .unwrap_err();

    // Move one second forward into vesting
    endpoint.forward_clock_unix_timestamp(1).await.unwrap();

    // We should now be able to redeem exactly one second worth of vested collateral
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        &claimer_allocation,
        &claimer_merkle_proof,
        &collateral_mint,
        vesting_per_second_collateral_amount,
    )
    .await
    .unwrap();

    // But not a single cent more
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        &claimer_allocation,
        &claimer_merkle_proof,
        &collateral_mint,
        1,
    )
    .await
    .unwrap_err();

    // Move time forward to the halfway point of vesting
    endpoint
        .forward_clock_unix_timestamp(u64::from(
            vesting_duration_seconds / 2 - 1,
        ))
        .await
        .unwrap();

    // We should now be able to redeem up to half of the vested collateral
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        &claimer_allocation,
        &claimer_merkle_proof,
        &collateral_mint,
        vesting_per_second_collateral_amount
            * (u64::from(vesting_duration_seconds) / 2)
            - vesting_per_second_collateral_amount,
    )
    .await
    .unwrap();

    // And not a single cent more
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        &claimer_allocation,
        &claimer_merkle_proof,
        &collateral_mint,
        1,
    )
    .await
    .unwrap_err();

    // Move time forward to long after the end of vesting
    endpoint
        .forward_clock_unix_timestamp(u64::from(
            vesting_duration_seconds / 2 + 1000,
        ))
        .await
        .unwrap();

    // We should now be able to redeem the rest of the vested collateral
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        &claimer_allocation,
        &claimer_merkle_proof,
        &collateral_mint,
        vesting_per_second_collateral_amount
            * (u64::from(vesting_duration_seconds) / 2),
    )
    .await
    .unwrap();

    // And not a single cent more
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        &claimer_allocation,
        &claimer_merkle_proof,
        &collateral_mint,
        1,
    )
    .await
    .unwrap_err();
}
