use anchor_spl::associated_token;
use psyche_solana_distributor::state::AirdropMetadata;
use psyche_solana_distributor::state::Allocation;
use psyche_solana_distributor::state::Vesting;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::distributor::AirdropMerkleTree;
use psyche_solana_tooling::distributor::find_pda_airdrop;
use psyche_solana_tooling::distributor::process_airdrop_create;
use psyche_solana_tooling::distributor::process_claim_create;
use psyche_solana_tooling::distributor::process_claim_redeem;

#[tokio::test]
pub async fn run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Test constants
    let payer = Keypair::new();
    let payer_lamports = 1_000_000_000;

    let airdrop_id = 42u64;
    let airdrop_authority = Keypair::new();

    let collateral_mint_authority = Keypair::new();
    let collateral_mint_decimals = 6;

    let mut claimers = vec![];
    for _ in 0..42 {
        claimers.push(Keypair::new());
    }
    let mut random_seed = 123;

    // Airdrop generated merkle tree content
    let mut expected_total_collateral = 0;
    let mut allocations = vec![];
    for nonce in 0..3 {
        for claimer in &claimers {
            let allocated_collateral_amount =
                u64::from(pseudo_rand_u32(&mut random_seed));
            expected_total_collateral += allocated_collateral_amount;
            allocations.push(Allocation {
                claimer: claimer.pubkey(),
                nonce,
                vesting: Vesting {
                    start_unix_timestamp: 0,
                    duration_seconds: 0,
                    end_collateral_amount: allocated_collateral_amount,
                },
            });
        }
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

    // Create the airdrop
    process_airdrop_create(
        &mut endpoint,
        &payer,
        airdrop_id,
        &airdrop_authority,
        airdrop_merkle_tree.root().unwrap(),
        AirdropMetadata {
            bytes: [0u8; AirdropMetadata::SIZE],
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

    // Fill up the airdrop's collateral vault
    let airdrop_collateral = associated_token::get_associated_token_address(
        &find_pda_airdrop(airdrop_id),
        &collateral_mint,
    );
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &collateral_mint,
            &collateral_mint_authority,
            &airdrop_collateral,
            expected_total_collateral,
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
                claimer_allocation.nonce,
                &claimer_allocation.vesting,
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
                claimer_allocation.nonce,
                &claimer_allocation.vesting,
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
        expected_total_collateral
    );
    assert_eq!(
        endpoint
            .get_spl_token_account(&airdrop_collateral)
            .await
            .unwrap()
            .unwrap()
            .amount,
        0
    );
}

fn pseudo_rand_u32(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    *seed
}
