use anchor_spl::associated_token;
use psyche_solana_distributor::state::AirdropMetadata;
use psyche_solana_distributor::state::Allocation;
use psyche_solana_distributor::state::MerkleHash;
use psyche_solana_distributor::state::Vesting;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::distributor::AirdropMerkleTree;
use psyche_solana_tooling::distributor::find_pda_airdrop;
use psyche_solana_tooling::distributor::process_airdrop_create;
use psyche_solana_tooling::distributor::process_airdrop_update;
use psyche_solana_tooling::distributor::process_airdrop_withdraw;
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
    let airdrop_authority_collateral_amount = 424242;

    let collateral_mint_authority = Keypair::new();
    let collateral_mint_decimals = 6;

    // Airdrop merkle tree content
    let claimer_total_collateral_amount = 323232;
    let claimer = Keypair::new();
    let airdrop_merkle_tree = AirdropMerkleTree::try_from(&vec![
        make_dummy_stranger_allocation(),
        make_dummy_stranger_allocation(),
        Allocation {
            claimer: claimer.pubkey(),
            nonce: 77,
            vesting: Vesting {
                start_unix_timestamp: 10,
                duration_seconds: 10,
                end_collateral_amount: claimer_total_collateral_amount,
            },
        },
        make_dummy_stranger_allocation(),
        make_dummy_stranger_allocation(),
    ])
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
        airdrop_merkle_tree.root().unwrap(),
        AirdropMetadata {
            bytes: [0u8; AirdropMetadata::SIZE],
        },
        &collateral_mint,
    )
    .await
    .unwrap();

    // Get the claimer's first allocation and matching proof combo
    let claimer_indexes = airdrop_merkle_tree
        .allocations_indexes_for_claimer(&claimer.pubkey())
        .unwrap();
    let claimer_allocation =
        airdrop_merkle_tree.allocations()[claimer_indexes[0]];
    let claimer_merkle_proof = airdrop_merkle_tree
        .proof_at_allocation_index(claimer_indexes[0])
        .unwrap();

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

    // Redeem should fail with an invalid proof
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_allocation.vesting,
        &[],
        &collateral_mint,
        0,
    )
    .await
    .unwrap_err();

    // Redeem should fail with an invalid vesting end_collateral_amount
    let mut claimer_vesting_corrupted1 = claimer_allocation.vesting;
    claimer_vesting_corrupted1.end_collateral_amount += 1;
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_vesting_corrupted1,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap_err();

    // Redeem should fail with an invalid vesting start_unix_timestamp
    let mut claimer_vesting_corrupted2 = claimer_allocation.vesting;
    claimer_vesting_corrupted2.start_unix_timestamp -= 1;
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_vesting_corrupted2,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap_err();

    // Redeem should fail with an invalid vesting duration_seconds
    let mut claimer_vesting_corrupted3 = claimer_allocation.vesting;
    claimer_vesting_corrupted3.duration_seconds -= 1;
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_vesting_corrupted3,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap_err();

    // Redeem should fail with an invalid allocation nonce
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce + 1,
        &claimer_allocation.vesting,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap_err();

    // Redeem should fail with an invalid claimer
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &Keypair::new(),
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_allocation.vesting,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap_err();

    // Redeem nothing should work with a valid input
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_allocation.vesting,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap();

    // Redeeming something should fail (not enough collateral deposited in airdrop)
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
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

    // The airdrop_authority deposits some collateral into the airdrop manually
    let airdrop_collateral = associated_token::get_associated_token_address(
        &find_pda_airdrop(airdrop_id),
        &collateral_mint,
    );
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
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_allocation.vesting,
        &claimer_merkle_proof,
        &collateral_mint,
        claimer_total_collateral_amount,
    )
    .await
    .unwrap();

    // Redeem anything more than that should fail
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
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

    // Freezing the airdrop should make redeeming fail
    endpoint.forward_clock_slot(1).await.unwrap();
    process_airdrop_update(
        &mut endpoint,
        &payer,
        airdrop_id,
        &airdrop_authority,
        Some(true),
        None,
        None,
    )
    .await
    .unwrap();
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_allocation.vesting,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap_err();

    // Unfreeze the airdrop should make redeeming possible again
    endpoint.forward_clock_slot(1).await.unwrap();
    process_airdrop_update(
        &mut endpoint,
        &payer,
        airdrop_id,
        &airdrop_authority,
        Some(false),
        None,
        None,
    )
    .await
    .unwrap();
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_allocation.vesting,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap();

    // Putting a dummy merkle root should make redeeming fail
    endpoint.forward_clock_slot(1).await.unwrap();
    process_airdrop_update(
        &mut endpoint,
        &payer,
        airdrop_id,
        &airdrop_authority,
        None,
        Some(MerkleHash::from_parts(&[b"dummy"])),
        None,
    )
    .await
    .unwrap();
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_allocation.vesting,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap_err();

    // Setting the merkle root to all zeroes should be rejected
    process_airdrop_update(
        &mut endpoint,
        &payer,
        airdrop_id,
        &airdrop_authority,
        None,
        Some(MerkleHash::default()),
        None,
    )
    .await
    .unwrap_err();

    // Restoring the correct merkle root should make redeeming work again
    endpoint.forward_clock_slot(1).await.unwrap();
    process_airdrop_update(
        &mut endpoint,
        &payer,
        airdrop_id,
        &airdrop_authority,
        None,
        Some(airdrop_merkle_tree.root().unwrap().clone()),
        None,
    )
    .await
    .unwrap();
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_allocation.vesting,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap();

    // Chaning the metadata should have no negative effect on claims
    endpoint.forward_clock_slot(1).await.unwrap();
    process_airdrop_update(
        &mut endpoint,
        &payer,
        airdrop_id,
        &airdrop_authority,
        None,
        None,
        Some(AirdropMetadata {
            bytes: [2u8; AirdropMetadata::SIZE],
        }),
    )
    .await
    .unwrap();
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
        &receiver_collateral,
        airdrop_id,
        claimer_allocation.nonce,
        &claimer_allocation.vesting,
        &claimer_merkle_proof,
        &collateral_mint,
        0,
    )
    .await
    .unwrap();

    // Check that after all these operations, redeeming past the cap still fails
    process_claim_redeem(
        &mut endpoint,
        &payer,
        &claimer,
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

    // Withdraw the rest of the collateral left from the airdrop
    let spill_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &Pubkey::new_unique(),
            &collateral_mint,
        )
        .await
        .unwrap();
    process_airdrop_withdraw(
        &mut endpoint,
        &payer,
        airdrop_id,
        &airdrop_authority,
        &spill_collateral,
        &collateral_mint,
        airdrop_authority_collateral_amount - claimer_total_collateral_amount,
    )
    .await
    .unwrap();

    // Check final balances
    assert_eq!(
        endpoint
            .get_spl_token_account(&receiver_collateral)
            .await
            .unwrap()
            .unwrap()
            .amount,
        claimer_total_collateral_amount
    );
    assert_eq!(
        endpoint
            .get_spl_token_account(&spill_collateral)
            .await
            .unwrap()
            .unwrap()
            .amount,
        airdrop_authority_collateral_amount - claimer_total_collateral_amount
    );
}

fn make_dummy_stranger_allocation() -> Allocation {
    Allocation {
        claimer: Pubkey::new_unique(),
        nonce: 666,
        vesting: Vesting {
            start_unix_timestamp: 0,
            duration_seconds: 0,
            end_collateral_amount: 888,
        },
    }
}
