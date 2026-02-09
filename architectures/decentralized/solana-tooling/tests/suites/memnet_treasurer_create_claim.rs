use psyche_coordinator::CoordinatorConfig;
use psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams;
use psyche_solana_coordinator::ClientId;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_create;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_grantor_update;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_join_run;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_participant_claim;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_participant_create;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_run_create;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_run_update;
use psyche_solana_treasurer::logic::RunCreateParams;
use psyche_solana_treasurer::logic::RunUpdateParams;
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
    let claimer1 = Keypair::new();
    let claimer2 = Keypair::new();

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
    let (run1, coordinator1_instance) = process_treasurer_run_create(
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
    let (run2, coordinator2_instance) = process_treasurer_run_create(
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

    // Update the runs' coordinator configs
    let dummy_config = CoordinatorConfig {
        warmup_time: 10,
        cooldown_time: 20,
        max_round_train_time: 888,
        round_witness_time: 42,
        min_clients: 1,
        init_min_clients: 1,
        global_batch_size_start: 1,
        global_batch_size_end: 42,
        global_batch_size_warmup_tokens: 0,
        verification_percent: 0,
        witness_nodes: 0,
        epoch_time: 999,
        total_steps: 100,
        waiting_for_members_extra_time: 3,
    };
    process_treasurer_run_update(
        &mut endpoint,
        &payer,
        &main_authority,
        &run1,
        &coordinator1_instance,
        &coordinator1_account,
        RunUpdateParams {
            metadata: None,
            config: Some(dummy_config),
            model: None,
            progress: None,
            epoch_earning_rate_total_shared: None,
            epoch_slashing_rate_per_client: None,
            paused: None,
            client_version: None,
        },
    )
    .await
    .unwrap();
    process_treasurer_run_update(
        &mut endpoint,
        &payer,
        &main_authority,
        &run2,
        &coordinator2_instance,
        &coordinator2_account,
        RunUpdateParams {
            metadata: None,
            config: Some(dummy_config),
            model: None,
            progress: None,
            epoch_earning_rate_total_shared: None,
            epoch_slashing_rate_per_client: None,
            paused: None,
            client_version: None,
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

    // Create the claimers ATA
    let claimer1_collateral1 = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &claimer1.pubkey(),
            &collateral1_mint,
        )
        .await
        .unwrap();
    let claimer1_collateral2 = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &claimer1.pubkey(),
            &collateral2_mint,
        )
        .await
        .unwrap();
    let claimer2_collateral1 = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &claimer2.pubkey(),
            &collateral1_mint,
        )
        .await
        .unwrap();
    let claimer2_collateral2 = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &claimer2.pubkey(),
            &collateral2_mint,
        )
        .await
        .unwrap();

    // Create the participation accounts
    process_treasurer_participant_create(
        &mut endpoint,
        &payer,
        &run1,
        &client1.pubkey(),
    )
    .await
    .unwrap();
    process_treasurer_participant_create(
        &mut endpoint,
        &payer,
        &run2,
        &client1.pubkey(),
    )
    .await
    .unwrap();
    process_treasurer_participant_create(
        &mut endpoint,
        &payer,
        &run1,
        &client2.pubkey(),
    )
    .await
    .unwrap();
    process_treasurer_participant_create(
        &mut endpoint,
        &payer,
        &run2,
        &client2.pubkey(),
    )
    .await
    .unwrap();

    // Try claiming before joining, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer1,
        &claimer1_collateral1,
        &collateral1_mint,
        &run1,
        &client1.pubkey(),
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();

    // Create and activate the join authorization for everyone
    let authorization = process_authorizer_authorization_create(
        &mut endpoint,
        &payer,
        &join_authority,
        &Pubkey::default(),
        &JOIN_RUN_AUTHORIZATION_SCOPE,
    )
    .await
    .unwrap();
    process_authorizer_authorization_grantor_update(
        &mut endpoint,
        &payer,
        &join_authority,
        &authorization,
        AuthorizationGrantorUpdateParams { active: true },
    )
    .await
    .unwrap();

    // Joining the runs
    process_coordinator_join_run(
        &mut endpoint,
        &payer,
        &client1,
        &authorization,
        &coordinator1_instance,
        &coordinator1_account,
        ClientId::new(client1.pubkey(), Default::default()),
        &claimer1.pubkey(),
    )
    .await
    .unwrap();
    process_coordinator_join_run(
        &mut endpoint,
        &payer,
        &client2,
        &authorization,
        &coordinator1_instance,
        &coordinator1_account,
        ClientId::new(client2.pubkey(), Default::default()),
        &claimer2.pubkey(),
    )
    .await
    .unwrap();
    process_coordinator_join_run(
        &mut endpoint,
        &payer,
        &client1,
        &authorization,
        &coordinator2_instance,
        &coordinator2_account,
        ClientId::new(client1.pubkey(), Default::default()),
        &claimer1.pubkey(),
    )
    .await
    .unwrap();
    process_coordinator_join_run(
        &mut endpoint,
        &payer,
        &client2,
        &authorization,
        &coordinator2_instance,
        &coordinator2_account,
        ClientId::new(client2.pubkey(), Default::default()),
        &claimer2.pubkey(),
    )
    .await
    .unwrap();

    // Try claiming nothing with proper inputs, it should work but do nothing
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer1,
        &claimer1_collateral1,
        &collateral1_mint,
        &run1,
        &client1.pubkey(),
        &coordinator1_account,
        0,
    )
    .await
    .unwrap();
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer2,
        &claimer2_collateral1,
        &collateral1_mint,
        &run1,
        &client2.pubkey(),
        &coordinator1_account,
        0,
    )
    .await
    .unwrap();
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer1,
        &claimer1_collateral2,
        &collateral2_mint,
        &run2,
        &client1.pubkey(),
        &coordinator2_account,
        0,
    )
    .await
    .unwrap();
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer2,
        &claimer2_collateral2,
        &collateral2_mint,
        &run2,
        &client2.pubkey(),
        &coordinator2_account,
        0,
    )
    .await
    .unwrap();

    // Try claiming something, it should fail since we earned nothing
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer1,
        &claimer1_collateral1,
        &collateral1_mint,
        &run1,
        &client1.pubkey(),
        &coordinator1_account,
        1,
    )
    .await
    .unwrap_err();

    // Try claiming using the wrong client, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer1,
        &claimer1_collateral1,
        &collateral1_mint,
        &run1,
        &client2.pubkey(), // Wrong client
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();

    // Try claiming using the wrong claimer, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer2, // Wrong claimer
        &claimer1_collateral1,
        &collateral1_mint,
        &run1,
        &client1.pubkey(),
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();

    // Try claiming using the wrong ATA, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer1,
        &claimer2_collateral1, // Wrong ATA
        &collateral1_mint,
        &run1,
        &client1.pubkey(),
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer1,
        &claimer1_collateral2, // Wrong ATA
        &collateral1_mint,
        &run1,
        &client1.pubkey(),
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();

    // Try claiming on the wrong mint, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer1,
        &claimer1_collateral1,
        &collateral2_mint, // Wrong mint
        &run1,
        &client1.pubkey(),
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();

    // Try claiming on the wrong run, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer1,
        &claimer1_collateral1,
        &collateral1_mint,
        &run2, // Wrong run
        &client1.pubkey(),
        &coordinator1_account,
        0,
    )
    .await
    .unwrap_err();

    // Try claiming on the wrong coordinator account, it should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &claimer1,
        &claimer1_collateral1,
        &collateral1_mint,
        &run1,
        &client1.pubkey(),
        &coordinator2_account, // Wrong coordinator account
        0,
    )
    .await
    .unwrap_err();

    // Noone should have been able to claim anything yet
    assert_amount(&mut endpoint, &claimer1_collateral1, 0).await;
    assert_amount(&mut endpoint, &claimer2_collateral2, 0).await;
    assert_amount(&mut endpoint, &claimer1_collateral1, 0).await;
    assert_amount(&mut endpoint, &claimer2_collateral2, 0).await;

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
