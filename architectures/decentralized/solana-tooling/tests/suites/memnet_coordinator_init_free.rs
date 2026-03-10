use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_coordinator::logic::InitCoordinatorParams;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_free;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_init;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;

#[tokio::test]
pub async fn run() {
    let mut endpoint = create_memnet_endpoint().await;

    // Create payer key and fund it
    let payer = Keypair::new();
    endpoint
        .request_airdrop(&payer.pubkey(), 5_000_000_000)
        .await
        .unwrap();

    // Run constants
    let main_authority = Keypair::new();
    let join_authority = Keypair::new();

    // Check the payer and main_authority balance before paying for the coordinator
    let payer_balance_start = endpoint
        .get_account_or_default(&payer.pubkey())
        .await
        .unwrap()
        .lamports;
    let main_authority_balance_start = endpoint
        .get_account_or_default(&main_authority.pubkey())
        .await
        .unwrap()
        .lamports;

    // create the empty pre-allocated coordinator_account
    let coordinator_account = endpoint
        .process_system_new_exempt(
            &payer,
            CoordinatorAccount::space_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Initialize coordinator
    let coordinator_instance = process_coordinator_init(
        &mut endpoint,
        &payer,
        &coordinator_account,
        InitCoordinatorParams {
            run_id: "this is a dummy run_id".to_string(),
            main_authority: main_authority.pubkey(),
            join_authority: join_authority.pubkey(),
            client_version: "test".to_string(),
        },
    )
    .await
    .unwrap();

    // Check the payer and authority balance after paying for the coordinator accounts
    let payer_balance_after = endpoint
        .get_account_or_default(&payer.pubkey())
        .await
        .unwrap()
        .lamports;
    let main_authority_balance_after = endpoint
        .get_account_or_default(&main_authority.pubkey())
        .await
        .unwrap()
        .lamports;

    // Check that balance mouvements match what we expect
    assert!(payer_balance_after < payer_balance_start);
    assert_eq!(main_authority_balance_after, main_authority_balance_start);

    // Check that the coordinator instance and account do actually exists now
    assert!(
        endpoint
            .get_account(&coordinator_instance)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        endpoint
            .get_account(&coordinator_account)
            .await
            .unwrap()
            .is_some()
    );

    // This spill account will be reimbursed for the costs of the rent
    let spill = Pubkey::new_unique();
    let spill_balance_before = endpoint
        .get_account_or_default(&spill)
        .await
        .unwrap()
        .lamports;

    // Free and close the coordinator account and instance
    process_coordinator_free(
        &mut endpoint,
        &payer,
        &main_authority,
        &spill,
        &coordinator_instance,
        &coordinator_account,
    )
    .await
    .unwrap();

    // Check all the keys balances at the end
    let payer_balance_final = endpoint
        .get_account_or_default(&payer.pubkey())
        .await
        .unwrap()
        .lamports;
    let main_authority_balance_final = endpoint
        .get_account_or_default(&main_authority.pubkey())
        .await
        .unwrap()
        .lamports;
    let spill_balance_final = endpoint
        .get_account_or_default(&spill)
        .await
        .unwrap()
        .lamports;

    // Check that we did in fact get reimbursed to the proper account
    assert_eq!(payer_balance_after - 5_000 * 2, payer_balance_final);
    assert_eq!(main_authority_balance_after, main_authority_balance_final);
    assert!(spill_balance_before < spill_balance_final);

    // Check that the coordinator account and instances were actually closed
    assert!(
        endpoint
            .get_account(&coordinator_instance)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        endpoint
            .get_account(&coordinator_account)
            .await
            .unwrap()
            .is_none()
    );
}
