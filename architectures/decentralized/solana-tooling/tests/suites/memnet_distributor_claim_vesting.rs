use anchor_spl::associated_token;
use psyche_solana_distributor::state::AirdropMetadata;
use psyche_solana_distributor::state::Allocation;
use psyche_solana_distributor::state::MerkleHash;
use psyche_solana_distributor::state::Vesting;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_endpoint::ToolboxEndpoint;

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
        airdrop_merkle_tree.root().unwrap(),
        AirdropMetadata {
            bytes: [0u8; AirdropMetadata::SIZE],
        },
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

    // Prepare common params for the claimer's claim redeem attempts
    let mut context = ClaimerClaimRedeemContext {
        endpoint,
        payer,
        claimer,
        receiver_collateral,
        airdrop_id,
        allocation: claimer_allocation,
        merkle_proof: claimer_merkle_proof,
        collateral_mint,
    };

    // Redeem nothing should work with a valid input
    do_redeem(&mut context, 0).await.unwrap();

    // Redeeming something before vesting start should fail
    do_redeem(&mut context, 1).await.unwrap_err();

    // Move time forward to exactly vesting start
    context
        .endpoint
        .forward_clock_unix_timestamp(u64::from(vesting_start_delay_seconds))
        .await
        .unwrap();

    // Redeeming something right at the start of vesting should still fail
    do_redeem(&mut context, 1).await.unwrap_err();

    // Move one second forward into vesting
    context
        .endpoint
        .forward_clock_unix_timestamp(1)
        .await
        .unwrap();

    // We should now be able to redeem exactly one second worth of vested collateral
    do_redeem(&mut context, vesting_per_second_collateral_amount)
        .await
        .unwrap();

    // But not a single cent more
    do_redeem(&mut context, 1).await.unwrap_err();

    // Move time forward to the halfway point of vesting
    context
        .endpoint
        .forward_clock_unix_timestamp(u64::from(
            vesting_duration_seconds / 2 - 1,
        ))
        .await
        .unwrap();

    // We should now be able to redeem up to half of the vested collateral
    do_redeem(
        &mut context,
        claimer_vesting.end_collateral_amount / 2
            - vesting_per_second_collateral_amount,
    )
    .await
    .unwrap();

    // And not a single cent more
    do_redeem(&mut context, 1).await.unwrap_err();

    // Move time forward to long after the end of vesting
    context
        .endpoint
        .forward_clock_unix_timestamp(u64::from(vesting_duration_seconds * 10))
        .await
        .unwrap();

    // We should now be able to redeem all the rest of the allocated collateral
    do_redeem(&mut context, claimer_vesting.end_collateral_amount / 2)
        .await
        .unwrap();

    // And not a single cent more
    do_redeem(&mut context, 1).await.unwrap_err();

    // And redeeming nothing should still work
    do_redeem(&mut context, 0).await.unwrap();

    // Check final balances
    assert_eq!(
        context
            .endpoint
            .get_spl_token_account(&context.receiver_collateral)
            .await
            .unwrap()
            .unwrap()
            .amount,
        claimer_vesting.end_collateral_amount
    );
    assert_eq!(
        context
            .endpoint
            .get_spl_token_account(&airdrop_collateral)
            .await
            .unwrap()
            .unwrap()
            .amount,
        airdrop_collateral_amount - claimer_vesting.end_collateral_amount
    );
}

struct ClaimerClaimRedeemContext {
    endpoint: ToolboxEndpoint,
    payer: Keypair,
    claimer: Keypair,
    receiver_collateral: Pubkey,
    airdrop_id: u64,
    allocation: Allocation,
    merkle_proof: Vec<MerkleHash>,
    collateral_mint: Pubkey,
}

async fn do_redeem(
    context: &mut ClaimerClaimRedeemContext,
    collateral_amount: u64,
) -> anyhow::Result<()> {
    process_claim_redeem(
        &mut context.endpoint,
        &context.payer,
        &context.claimer,
        &context.receiver_collateral,
        context.airdrop_id,
        context.allocation.nonce,
        &context.allocation.vesting,
        &context.merkle_proof,
        &context.collateral_mint,
        collateral_amount,
    )
    .await
}
