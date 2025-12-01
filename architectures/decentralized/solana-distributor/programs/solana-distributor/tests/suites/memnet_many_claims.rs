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
    let airdrop_authority_collateral_amount = 424242;

    let collateral_mint_authority = Keypair::new();
    let collateral_mint_decimals = 6;

    // Airdrop merkle tree content
    let mut expected_total_claimed_collateral = 0;
    let per_claimer_collateral_amount = 42;
    let mut claimers = vec![];
    let mut allocations = vec![];
    for i in 0..42 {
        let claimer = Keypair::new();
        allocations.push(Allocation {
            claimer: claimer.pubkey(),
            nonce: 0,
            vesting: Vesting {
                start_unix_timestamp: 0,
                duration_seconds: 0,
                end_collateral_amount: per_claimer_collateral_amount,
            },
        });
        expected_total_claimed_collateral += per_claimer_collateral_amount;
        if i % 3 == 0 {
            allocations.push(Allocation {
                claimer: claimer.pubkey(),
                nonce: 1,
                vesting: Vesting {
                    start_unix_timestamp: 0,
                    duration_seconds: 0,
                    end_collateral_amount: per_claimer_collateral_amount * 3,
                },
            });
            expected_total_claimed_collateral +=
                per_claimer_collateral_amount * 3;
        }
        claimers.push(claimer);
    }
    let airdrop_merkle_tree =
        AirdropMerkleTree::try_from(&allocations).unwrap();

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

    // Give the airdrop_authority some collateral
    let airdrop_authority_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &airdrop_authority.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();
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

    // Create a wallet that will receive the airdrop's claimed collateral
    let receiver_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &Pubkey::new_unique(),
            &collateral_mint,
        )
        .await
        .unwrap();

    // Give the airdrop enough collateral
    let airdrop = find_pda_airdrop(airdrop_id);
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

    // Redeem full amount for everything should work
    for claimer in &claimers {
        for allocation_index in airdrop_merkle_tree
            .allocations_indexes_for_claimer(&claimer.pubkey())
            .unwrap()
        {
            // First prepare the claim PDA
            let claimer_allocation =
                airdrop_merkle_tree.allocations()[allocation_index];
            let claimer_merkle_proof = airdrop_merkle_tree
                .proof_at_allocation_index(allocation_index)
                .unwrap();
            process_claim_create(
                &mut endpoint,
                &payer,
                claimer,
                airdrop_id,
                claimer_allocation.nonce,
            )
            .await
            .unwrap();
            // Then redeem the whole thing at once, it should work
            process_claim_redeem(
                &mut endpoint,
                &payer,
                claimer,
                &receiver_collateral,
                airdrop_id,
                &claimer_allocation,
                &claimer_merkle_proof,
                &collateral_mint,
                claimer_allocation.vesting.end_collateral_amount,
            )
            .await
            .unwrap();
            // Then redeeming anything past this should fail
            process_claim_redeem(
                &mut endpoint,
                &payer,
                claimer,
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
    }

    // Check final balances
    assert_eq!(
        endpoint
            .get_spl_token_account(&receiver_collateral)
            .await
            .unwrap()
            .unwrap()
            .amount,
        expected_total_claimed_collateral
    );
    assert_eq!(
        endpoint
            .get_spl_token_account(&airdrop_collateral)
            .await
            .unwrap()
            .unwrap()
            .amount,
        airdrop_authority_collateral_amount - expected_total_claimed_collateral
    );
}
