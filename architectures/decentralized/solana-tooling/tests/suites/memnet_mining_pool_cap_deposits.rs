use psyche_solana_mining_pool::state::PoolMetadata;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::mining_pool::process_lender_create;
use psyche_solana_tooling::mining_pool::process_lender_deposit;
use psyche_solana_tooling::mining_pool::process_pool_create;
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

    let collateral_mint_authority = Keypair::new();
    let collateral_mint_decimals = 6;

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

    // Give the user some collateral
    let user_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &user.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();
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

    // Prepare the lender account for our user
    process_lender_create(&mut endpoint, &payer, &user, pool_index)
        .await
        .unwrap();

    // Deposit should fail until we set the proper deposit cap
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        pool_index,
        &collateral_mint,
        1,
    )
    .await
    .unwrap_err();

    // Raise the cap so that we can deposit some amount
    process_pool_update(
        &mut endpoint,
        &payer,
        pool_index,
        &pool_authority,
        Some(1),
        None,
        None,
    )
    .await
    .unwrap();

    // Even if we set the cap, depositing too much should fail
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        pool_index,
        &collateral_mint,
        user_collateral_amount,
    )
    .await
    .unwrap_err();

    // Depositing the proper amount should work
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        pool_index,
        &collateral_mint,
        1,
    )
    .await
    .unwrap();

    // Depositing past that proper amount should fail
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        pool_index,
        &collateral_mint,
        1,
    )
    .await
    .unwrap_err();

    // Raise the cap so that we can deposit the whole thing
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

    // Deposit the whole remaining amount should succeed now that the cap is high
    process_lender_deposit(
        &mut endpoint,
        &payer,
        &user,
        &user_collateral,
        pool_index,
        &collateral_mint,
        user_collateral_amount - 1, // we already deposited 1
    )
    .await
    .unwrap();
}
