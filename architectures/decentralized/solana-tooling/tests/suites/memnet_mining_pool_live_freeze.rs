use psyche_solana_mining_pool::find_pool;
use psyche_solana_mining_pool::state::PoolMetadata;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::mining_pool::process_lender_claim;
use psyche_solana_tooling::mining_pool::process_lender_create;
use psyche_solana_tooling::mining_pool::process_lender_deposit;
use psyche_solana_tooling::mining_pool::process_pool_claimable;
use psyche_solana_tooling::mining_pool::process_pool_create;
use psyche_solana_tooling::mining_pool::process_pool_extract;
use psyche_solana_tooling::mining_pool::process_pool_update;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

#[tokio::test]
pub async fn run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Test constants
    let payer = Keypair::new();
    let payer_lamports = 1_000_000_000;

    let pool_index = 42u64;
    let pool_authority = Keypair::new();
    let pool_authority_redeemable_amount = 424242;

    let collateral_mint_authority = Keypair::new();
    let collateral_mint_decimals = 6;

    let redeemable_mint_authority = Keypair::new();
    let redeemable_mint_decimals = 9;

    let user = Keypair::new();
    let user_collateral_amount = 99999;

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
    let user_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &user.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();
    let pool_authority_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &pool_authority.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();

    // Give the user some collateral
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &collateral_mint,
            &collateral_mint_authority,
            &user_collateral,
            user_collateral_amount,
        )
        .await
        .unwrap();

    // Create the global redeemable mint
    let redeemable_mint = endpoint
        .process_spl_token_mint_new(
            &payer,
            &redeemable_mint_authority.pubkey(),
            None,
            redeemable_mint_decimals,
        )
        .await
        .unwrap();

    // Create redeemable ATAs
    let user_redeemable = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &user.pubkey(),
            &redeemable_mint,
        )
        .await
        .unwrap();
    let pool_authority_redeemable = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &pool_authority.pubkey(),
            &redeemable_mint,
        )
        .await
        .unwrap();

    // Give the pool_authority some redeemable
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &redeemable_mint,
            &redeemable_mint_authority,
            &pool_authority_redeemable,
            pool_authority_redeemable_amount,
        )
        .await
        .unwrap();

    // Create the funding pool
    process_pool_create(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        PoolMetadata {
            length: 0,
            bytes: [0u8; PoolMetadata::BYTES],
        },
        &collateral_mint,
    )
    .await
    .unwrap();

    // Make the pool claimable using the redeemable mint
    process_pool_claimable(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        &redeemable_mint,
    )
    .await
    .unwrap();

    // Set the pool deposit cap
    process_pool_update(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        Some(user_collateral_amount),
        None,
        None,
    )
    .await
    .unwrap();

    // Find the pool's ATA
    let pool = find_pool(pool_index);
    let pool_redeemable = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &pool,
            &redeemable_mint,
        )
        .await
        .unwrap();

    // Send the redeemable to the pool
    endpoint
        .process_spl_token_transfer(
            &payer,
            &pool_authority,
            &pool_authority_redeemable,
            &pool_redeemable,
            pool_authority_redeemable_amount,
        )
        .await
        .unwrap();

    // Prepare the lender account for our user
    process_lender_create(&mut endpoint, &payer, &user, pool_index)
        .await
        .unwrap();

    // Pool is ready to deposit (deposit half should work)
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        pool_index,
        &collateral_mint,
        user_collateral_amount / 2,
    )
    .await
    .unwrap();

    // Claiming should work for now
    process_lender_claim(
        &mut endpoint,
        &payer,
        &user,
        &user_redeemable,
        pool_index,
        &redeemable_mint,
        pool_authority_redeemable_amount / 2,
    )
    .await
    .unwrap();

    // Freeze the pool, any action after that should fail until un-freeze
    process_pool_update(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        None,
        Some(true),
        None,
    )
    .await
    .unwrap();

    // Extract should fail
    process_pool_extract(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        &pool_authority_collateral,
        &collateral_mint,
        user_collateral_amount / 2,
    )
    .await
    .unwrap_err();

    // Deposit should fail
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        pool_index,
        &collateral_mint,
        user_collateral_amount / 2,
    )
    .await
    .unwrap_err();

    // Claiming should fail
    process_lender_claim(
        &mut endpoint,
        &payer,
        &user,
        &user_redeemable,
        pool_index,
        &redeemable_mint,
        pool_authority_redeemable_amount / 2,
    )
    .await
    .unwrap_err();

    // Un-freeze the pool, all the above should work again
    process_pool_update(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        None,
        Some(false),
        None,
    )
    .await
    .unwrap();

    // Extract should now succeed
    process_pool_extract(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        &pool_authority_collateral,
        &collateral_mint,
        user_collateral_amount / 2,
    )
    .await
    .unwrap();

    // Depositing should now succeed with partial amounts
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        pool_index,
        &collateral_mint,
        user_collateral_amount / 3,
    )
    .await
    .unwrap();
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        pool_index,
        &collateral_mint,
        user_collateral_amount / 6,
    )
    .await
    .unwrap();

    // Claiming should now succeed with partial amounts
    process_lender_claim(
        &mut endpoint,
        &payer,
        &user,
        &user_redeemable,
        pool_index,
        &redeemable_mint,
        pool_authority_redeemable_amount / 3,
    )
    .await
    .unwrap();
    process_lender_claim(
        &mut endpoint,
        &payer,
        &user,
        &user_redeemable,
        pool_index,
        &redeemable_mint,
        pool_authority_redeemable_amount / 6,
    )
    .await
    .unwrap();

    // Extract should still succeed for the amounts just deposited, using partial amounts
    process_pool_extract(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        &pool_authority_collateral,
        &collateral_mint,
        user_collateral_amount / 3,
    )
    .await
    .unwrap();
    process_pool_extract(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        &pool_authority_collateral,
        &collateral_mint,
        user_collateral_amount / 6,
    )
    .await
    .unwrap();
}
