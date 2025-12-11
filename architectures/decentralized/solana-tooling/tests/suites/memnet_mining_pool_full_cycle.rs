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
    let pool_authority_redeemable_amount = 42_000;

    let collateral_mint_authority = Keypair::new();
    let collateral_mint_decimals = 6;

    let redeemable_mint_authority = Keypair::new();
    let redeemable_mint_decimals = 6;

    let user1 = Keypair::new();
    let user2 = Keypair::new();

    let user1_collateral_amount = 900;
    let user2_collateral_amount = 600;

    let user1_redeemable_amount = pool_authority_redeemable_amount
        * user1_collateral_amount
        / (user1_collateral_amount + user2_collateral_amount);
    let user2_redeemable_amount = pool_authority_redeemable_amount
        * user2_collateral_amount
        / (user1_collateral_amount + user2_collateral_amount);

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

    // Set the deposit cap so that future deposit will work
    process_pool_update(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        Some(user1_collateral_amount + user2_collateral_amount),
        None,
        None,
    )
    .await
    .unwrap();

    // Give the User1 some collateral
    let user1_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &user1.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &collateral_mint,
            &collateral_mint_authority,
            &user1_collateral,
            user1_collateral_amount,
        )
        .await
        .unwrap();

    // Prepare the lender account for User1
    process_lender_create(&mut endpoint, &payer, &user1, pool_index)
        .await
        .unwrap();

    // Deposit a small amount of collateral in the pool
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user1,
        &user1_collateral,
        pool_index,
        &collateral_mint,
        user1_collateral_amount / 4,
    )
    .await
    .unwrap();

    // Deposit all remaining the User1's collateral
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user1,
        &user1_collateral,
        pool_index,
        &collateral_mint,
        user1_collateral_amount * 3 / 4,
    )
    .await
    .unwrap();

    // The wrong authority should not be able to extract collateral from the pool
    let payer_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &payer.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();
    process_pool_extract(
        &mut endpoint,
        &payer,
        pool_index,
        &payer,
        &payer_collateral,
        &collateral_mint,
        1,
    )
    .await
    .unwrap_err();

    // The correct authority should be able to withdraw the collateral from the pool
    let pool_authority_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &pool_authority.pubkey(),
            &collateral_mint,
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
        user1_collateral_amount,
    )
    .await
    .unwrap();

    // Give the User2 some collateral
    let user2_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &user2.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &collateral_mint,
            &collateral_mint_authority,
            &user2_collateral,
            user2_collateral_amount,
        )
        .await
        .unwrap();

    // Prepare the lender account for User2
    process_lender_create(&mut endpoint, &payer, &user2, pool_index)
        .await
        .unwrap();

    // Deposit all the User2's collateral
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user2,
        &user2_collateral,
        pool_index,
        &collateral_mint,
        user2_collateral_amount,
    )
    .await
    .unwrap();

    // The authority should be able to withdraw the collateral from the pool
    process_pool_extract(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        &pool_authority_collateral,
        &collateral_mint,
        user2_collateral_amount,
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

    // Give some redeemable to the authority
    let pool_authority_redeemable = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &pool_authority.pubkey(),
            &redeemable_mint,
        )
        .await
        .unwrap();
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

    // Repay half of the redeemable back into the pool
    endpoint
        .process_spl_token_transfer(
            &payer,
            &pool_authority,
            &pool_authority_redeemable,
            &pool_redeemable,
            pool_authority_redeemable_amount / 2,
        )
        .await
        .unwrap();

    // User1 can now claim half of its allocated redeemable
    let user1_redeemable = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &user1.pubkey(),
            &redeemable_mint,
        )
        .await
        .unwrap();
    process_lender_claim(
        &mut endpoint,
        &payer,
        &user1,
        &user1_redeemable,
        pool_index,
        &redeemable_mint,
        user1_redeemable_amount / 2,
    )
    .await
    .unwrap();

    // Claiming too much should fail
    process_lender_claim(
        &mut endpoint,
        &payer,
        &user1,
        &user1_redeemable,
        pool_index,
        &redeemable_mint,
        1,
    )
    .await
    .unwrap_err();

    // User2 can now claim half of its allocated redeemable
    let user2_redeemable = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &user2.pubkey(),
            &redeemable_mint,
        )
        .await
        .unwrap();
    process_lender_claim(
        &mut endpoint,
        &payer,
        &user2,
        &user2_redeemable,
        pool_index,
        &redeemable_mint,
        user2_redeemable_amount / 2,
    )
    .await
    .unwrap();

    // Repay the remaining of the redeemable back into the pool in small bits
    endpoint
        .process_spl_token_transfer(
            &payer,
            &pool_authority,
            &pool_authority_redeemable,
            &pool_redeemable,
            pool_authority_redeemable_amount / 3,
        )
        .await
        .unwrap();
    endpoint
        .process_spl_token_transfer(
            &payer,
            &pool_authority,
            &pool_authority_redeemable,
            &pool_redeemable,
            pool_authority_redeemable_amount / 6,
        )
        .await
        .unwrap();

    // The users can now claim their remaining reward in any fraction
    process_lender_claim(
        &mut endpoint,
        &payer,
        &user1,
        &user1_redeemable,
        pool_index,
        &redeemable_mint,
        user1_redeemable_amount / 3,
    )
    .await
    .unwrap();
    process_lender_claim(
        &mut endpoint,
        &payer,
        &user1,
        &user1_redeemable,
        pool_index,
        &redeemable_mint,
        user1_redeemable_amount / 6,
    )
    .await
    .unwrap();

    process_lender_claim(
        &mut endpoint,
        &payer,
        &user2,
        &user2_redeemable,
        pool_index,
        &redeemable_mint,
        user2_redeemable_amount / 3,
    )
    .await
    .unwrap();
    process_lender_claim(
        &mut endpoint,
        &payer,
        &user2,
        &user2_redeemable,
        pool_index,
        &redeemable_mint,
        user2_redeemable_amount / 6,
    )
    .await
    .unwrap();
}
