use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_participant_claim;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_participant_create;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_run_create;
use psyche_solana_treasurer::logic::RunCreateParams;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_toolbox_endpoint::ToolboxEndpoint;

#[tokio::test]
pub async fn run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Create payer key and fund it
    let payer = Keypair::new();
    endpoint
        .request_airdrop(&payer.pubkey(), 5_000_000_000)
        .await
        .unwrap();

    // Constants
    let mint_authority = Keypair::new();
    let main_authority = Keypair::new();
    let join_authority = Keypair::new();
    let client1 = Keypair::new();
    let client2 = Keypair::new();

    // Prepare the collateral mints
    let collateral1_mint = endpoint
        .process_spl_token_mint_new(&payer, &mint_authority.pubkey(), None, 6)
        .await
        .unwrap();
    let collateral2_mint = endpoint
        .process_spl_token_mint_new(&payer, &mint_authority.pubkey(), None, 6)
        .await
        .unwrap();

    // Create the empty pre-allocated coordinator accounts
    let coordinator1_account = endpoint
        .process_system_new_exempt(
            &payer,
            CoordinatorAccount::space_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();
    let coordinator2_account = endpoint
        .process_system_new_exempt(
            &payer,
            CoordinatorAccount::space_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Create the runs (it should init the underlying coordinators)
    let (run1, _) = process_treasurer_run_create(
        &mut endpoint,
        &payer,
        &collateral1_mint,
        &coordinator1_account,
        RunCreateParams {
            index: 41,
            run_id: "This is my run's dummy run_id1".to_string(),
            main_authority: main_authority.pubkey(),
            join_authority: join_authority.pubkey(),
            client_version: "latest".to_string(),
        },
    )
    .await
    .unwrap();
    let (run2, _) = process_treasurer_run_create(
        &mut endpoint,
        &payer,
        &collateral2_mint,
        &coordinator2_account,
        RunCreateParams {
            index: 42,
            run_id: "This is my run's dummy run_id2".to_string(),
            main_authority: main_authority.pubkey(),
            join_authority: join_authority.pubkey(),
            client_version: "latest".to_string(),
        },
    )
    .await
    .unwrap();

    // Get the run's collateral vaults
    let run1_collateral1 = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &run1,
            &collateral1_mint,
        )
        .await
        .unwrap();
    let run2_collateral2 = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &run1,
            &collateral2_mint,
        )
        .await
        .unwrap();

    // Give the runs some collaterals
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &collateral1_mint,
            &mint_authority,
            &run1_collateral1,
            1_000_000_000_000,
        )
        .await
        .unwrap();
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &collateral2_mint,
            &mint_authority,
            &run2_collateral2,
            1_000_000_000_000,
        )
        .await
        .unwrap();

    // Create the clients ATA
    let client1_collateral1 = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &client1.pubkey(),
            &collateral1_mint,
        )
        .await
        .unwrap();
    let client1_collateral2 = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &client1.pubkey(),
            &collateral2_mint,
        )
        .await
        .unwrap();
    let client2_collateral1 = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &client2.pubkey(),
            &collateral1_mint,
        )
        .await
        .unwrap();
    let client2_collateral2 = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &client2.pubkey(),
            &collateral2_mint,
        )
        .await
        .unwrap();

    // Create the participation accounts
    process_treasurer_participant_create(
        &mut endpoint,
        &payer,
        &client1,
        &run1,
    )
    .await
    .unwrap();
    process_treasurer_participant_create(
        &mut endpoint,
        &payer,
        &client1,
        &run2,
    )
    .await
    .unwrap();
    process_treasurer_participant_create(
        &mut endpoint,
        &payer,
        &client2,
        &run1,
    )
    .await
    .unwrap();
    process_treasurer_participant_create(
        &mut endpoint,
        &payer,
        &client2,
        &run2,
    )
    .await
    .unwrap();

    // Try claiming nothing with proper inputs, it should work but do nothing
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client1,
        &client1_collateral1,
        &collateral1_mint,
        &run1,
        &coordinator1_account,
        0,
    )
    .await
    .unwrap();
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client2,
        &client2_collateral1,
        &collateral1_mint,
        &run1,
        &coordinator1_account,
        0,
    )
    .await
    .unwrap();
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client1,
        &client1_collateral2,
        &collateral2_mint,
        &run2,
        &coordinator2_account,
        0,
    )
    .await
    .unwrap();
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client2,
        &client2_collateral2,
        &collateral2_mint,
        &run2,
        &coordinator2_account,
        0,
    )
    .await
    .unwrap();

    // Try claiming something, it should fail since we earned nothing
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client1,
        &client1_collateral1,
        &collateral1_mint,
        &run1,
        &coordinator1_account,
        1,
    )
    .await
    .unwrap_err();

    // Try claiming using the wrong owner, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client2,
        &client1_collateral1,
        &collateral1_mint,
        &run1,
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();

    // Try claiming using the wrong ATA, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client1,
        &client2_collateral1,
        &collateral1_mint,
        &run1,
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client1,
        &client1_collateral2,
        &collateral1_mint,
        &run1,
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();

    // Try claiming on the wrong mint, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client1,
        &client1_collateral1,
        &collateral2_mint,
        &run1,
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();

    // Try claiming on the wrong run, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client1,
        &client1_collateral1,
        &collateral1_mint,
        &run2,
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();

    // Try claiming on the wrong coordinator account, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client1,
        &client1_collateral1,
        &collateral1_mint,
        &run1,
        &coordinator2_account,
        0,
    )
    .await
    .unwrap_err();

    // Noone should have been able to claim anything yet
    assert_amount(&mut endpoint, &client1_collateral1, 0).await;
    assert_amount(&mut endpoint, &client2_collateral2, 0).await;
    assert_amount(&mut endpoint, &client1_collateral1, 0).await;
    assert_amount(&mut endpoint, &client2_collateral2, 0).await;

    // All the runs collateral should still be intact
    assert_amount(&mut endpoint, &run1_collateral1, 1_000_000_000_000).await;
    assert_amount(&mut endpoint, &run2_collateral2, 1_000_000_000_000).await;
}

async fn assert_amount(
    endpoint: &mut ToolboxEndpoint,
    account: &Pubkey,
    expected_amount: u64,
) {
    assert_eq!(
        endpoint
            .get_spl_token_account(account)
            .await
            .unwrap()
            .unwrap()
            .amount,
        expected_amount,
    );
}
