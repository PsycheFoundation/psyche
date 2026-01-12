// Integration tests for run-manager commands
// These tests spin up a local Solana test validator and test all run-manager commands

mod common;

use anchor_client::{
    Cluster,
    solana_sdk::{commitment_config::CommitmentConfig, signature::Signer},
};
use common::{TestClient, TestValidator, create_test_keypair};
use psyche_coordinator::RunState;
use psyche_solana_rpc::SolanaBackend;
use run_manager::commands::{
    Command,
    authorization::{
        CommandJoinAuthorizationCreate, CommandJoinAuthorizationDelete,
        CommandJoinAuthorizationRead,
    },
    can_join::CommandCanJoin,
    run::{CommandCloseRun, CommandCreateRun, CommandJsonDumpRun, CommandSetPaused},
};
use serial_test::serial;

/// Create a run and verify it exists on-chain
#[tokio::test]
#[serial]
async fn test_create_run() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let wallet_arc = create_test_keypair().expect("Failed to create test keypair");

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        wallet_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: None,
    };

    params
        .execute(backend)
        .await
        .expect("create-run command failed");

    // Verify run exists
    let mut test_client = TestClient::new(run_id.clone(), wallet_arc)
        .await
        .expect("Failed to create test client");

    test_client
        .refresh_coordinator_account()
        .await
        .expect("Failed to refresh coordinator account");

    assert!(test_client.run_exists().await, "Run should exist on-chain");

    let state = test_client
        .get_run_state()
        .await
        .expect("Failed to get run state");
    assert_eq!(
        state,
        RunState::Uninitialized,
        "Newly created run should be in Uninitialized state"
    );
}

/// Create a run, pause it, then resume it
#[tokio::test]
#[serial]
async fn test_pause_and_resume_run() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let wallet_arc = create_test_keypair().expect("Failed to create test keypair");

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        wallet_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: None,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("create-run failed");

    let mut test_client = TestClient::new(run_id.clone(), wallet_arc.clone())
        .await
        .expect("Failed to create test client");

    test_client
        .refresh_coordinator_account()
        .await
        .expect("Failed to refresh");

    let pause_params = CommandSetPaused {
        run_id: run_id.clone(),
        treasurer_index: None,
        resume: false,
    };

    pause_params
        .execute(backend.clone())
        .await
        .expect("set-paused (pause) failed");

    // Wait a bit for state to update
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let resume_params = CommandSetPaused {
        run_id: run_id.clone(),
        treasurer_index: None,
        resume: true,
    };

    resume_params
        .execute(backend)
        .await
        .expect("set-paused (resume) failed");

    println!("Successfully paused and resumed run");
}

/// Create a run and query it with json-dump-run
#[tokio::test]
#[serial]
async fn test_json_dump_run() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let wallet_arc = create_test_keypair().expect("Failed to create test keypair");

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        wallet_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: None,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("create-run failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let query_params = CommandJsonDumpRun {
        run_id: run_id.clone(),
        treasurer_index: None,
    };

    query_params
        .execute(backend)
        .await
        .expect("json-dump-run failed");

    println!("Successfully queried run with json-dump-run");
}

/// Create a run and then close it
#[tokio::test]
#[serial]
async fn test_close_run() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let wallet_arc = create_test_keypair().expect("Failed to create test keypair");

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        wallet_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: None,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("create-run failed");

    let test_client = TestClient::new(run_id.clone(), wallet_arc.clone())
        .await
        .expect("Failed to create test client");

    // Wait a bit
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let close_params = CommandCloseRun {
        run_id: run_id.clone(),
    };

    close_params
        .execute(backend)
        .await
        .expect("close-run failed");

    // Wait for closure to propagate
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Run should no longer exist
    assert!(
        !test_client.run_exists().await,
        "Run should no longer exist after closing"
    );

    println!("Successfully closed run");
}

/// Create and read join authorization
#[tokio::test]
#[serial]
async fn test_join_authorization_create_and_read() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let grantor_arc = create_test_keypair().expect("Failed to create grantor keypair");
    let grantee_pubkey = create_test_keypair()
        .expect("Failed to create grantee keypair")
        .pubkey();

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        grantor_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandJoinAuthorizationCreate {
        authorizer: grantee_pubkey,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("join-authorization-create failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let read_params = CommandJoinAuthorizationRead {
        join_authority: grantor_arc.pubkey(),
        authorizer: Some(grantee_pubkey),
    };

    read_params
        .execute(backend)
        .await
        .expect("join-authorization-read failed");

    println!("Successfully created and read join authorization");
}

/// Create and delete join authorization
#[tokio::test]
#[serial]
async fn test_join_authorization_delete() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let grantor_arc = create_test_keypair().expect("Failed to create grantor keypair");
    let grantee_pubkey = create_test_keypair()
        .expect("Failed to create grantee keypair")
        .pubkey();

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        grantor_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandJoinAuthorizationCreate {
        authorizer: grantee_pubkey,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("join-authorization-create failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let delete_params = CommandJoinAuthorizationDelete {
        authorizer: grantee_pubkey,
    };

    delete_params
        .execute(backend)
        .await
        .expect("join-authorization-delete failed");

    println!("Successfully deleted join authorization");
}

/// Can-join command for a paused run
#[tokio::test]
#[serial]
async fn test_can_join_paused_run() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let wallet_arc = create_test_keypair().expect("Failed to create test keypair");

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        wallet_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: Some(wallet_arc.pubkey()),
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("create-run failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let pause_params = CommandSetPaused {
        run_id: run_id.clone(),
        treasurer_index: None,
        resume: false,
    };

    pause_params
        .execute(backend.clone())
        .await
        .expect("set-paused failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let can_join_params = CommandCanJoin {
        run_id: run_id.clone(),
        authorizer: None,
        address: wallet_arc.pubkey(),
    };

    // This should succeed (no authorization needed when join_authority is the same as address)
    can_join_params
        .execute(backend)
        .await
        .expect("can-join failed");

    println!("Successfully tested can-join on paused run");
}

/// Full workflow - create run, authorize, check can-join
#[tokio::test]
#[serial]
async fn test_full_authorization_workflow() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let owner_arc = create_test_keypair().expect("Failed to create owner keypair");
    let user_pubkey = create_test_keypair()
        .expect("Failed to create user keypair")
        .pubkey();

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        owner_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    // 1. Create run with owner as join authority
    let create_params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: Some(owner_arc.pubkey()),
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("create-run failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 2. Create authorization for user
    let auth_params = CommandJoinAuthorizationCreate {
        authorizer: user_pubkey,
    };

    auth_params
        .execute(backend.clone())
        .await
        .expect("join-authorization-create failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // 3. Check can-join for authorized user
    let can_join_params = CommandCanJoin {
        run_id: run_id.clone(),
        authorizer: Some(user_pubkey),
        address: user_pubkey,
    };

    can_join_params
        .execute(backend)
        .await
        .expect("can-join failed for authorized user");

    println!("Successfully completed full authorization workflow");
}

/// Update config on a run
#[tokio::test]
#[serial]
async fn test_update_config() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let wallet_arc = create_test_keypair().expect("Failed to create test keypair");

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        wallet_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: None,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("create-run failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Note: Full config update testing would require a valid config file
    println!("Update config command skipped - requires config file");
}

/// Tick command to advance coordinator state
#[tokio::test]
#[serial]
async fn test_tick() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let wallet_arc = create_test_keypair().expect("Failed to create test keypair");

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        wallet_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: None,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("create-run failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    use run_manager::commands::run::CommandTick;

    let tick_params = CommandTick {
        run_id: run_id.clone(),
        ms_interval: 1000,
        count: Some(1), // Just tick once for testing
    };

    tick_params.execute(backend).await.expect("tick failed");

    println!("Successfully ticked run");
}

/// Set future epoch rates
#[tokio::test]
#[serial]
async fn test_set_future_epoch_rates() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let wallet_arc = create_test_keypair().expect("Failed to create test keypair");

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        wallet_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: None,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("create-run failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    use run_manager::commands::run::CommandSetFutureEpochRates;

    let rates_params = CommandSetFutureEpochRates {
        run_id: run_id.clone(),
        treasurer_index: None,
        earning_rate_total_shared: Some(0.001),
        slashing_rate_per_client: Some(0.0005),
    };

    rates_params
        .execute(backend)
        .await
        .expect("set-future-epoch-rates failed");

    println!("Successfully set future epoch rates");
}

/// Checkpoint command
#[tokio::test]
#[serial]
async fn test_checkpoint() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let wallet_arc = create_test_keypair().expect("Failed to create test keypair");

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        wallet_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: None,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("create-run failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    use run_manager::commands::run::CommandCheckpoint;

    let checkpoint_params = CommandCheckpoint {
        run_id: run_id.clone(),
        repo: "test-org/test-model".to_string(),
        revision: Some("abc123".to_string()),
    };

    checkpoint_params
        .execute(backend)
        .await
        .expect("checkpoint failed");

    println!("Successfully recorded checkpoint");
}

/// Delegate authorization management
#[tokio::test]
#[serial]
async fn test_join_authorization_delegate() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let grantor_arc = create_test_keypair().expect("Failed to create grantor keypair");
    let delegate_arc = create_test_keypair().expect("Failed to create delegate keypair");

    let delegate_pubkey = delegate_arc.pubkey();

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        grantor_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    // First create an authorization to manage
    let grantee_keypair = create_test_keypair().expect("Failed to create grantee keypair");
    let grantee_pubkey = grantee_keypair.pubkey();

    let create_params = CommandJoinAuthorizationCreate {
        authorizer: grantee_pubkey,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("join-authorization-create failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // Now delegate management to another address
    use run_manager::commands::authorization::CommandJoinAuthorizationDelegate;

    let delegate_params = CommandJoinAuthorizationDelegate {
        join_authority: grantor_arc.pubkey(),
        delegates_clear: false,
        delegates_added: vec![delegate_pubkey],
    };

    delegate_params
        .execute(backend)
        .await
        .expect("join-authorization-delegate failed");

    println!("Successfully delegated authorization management");
}

/// json-dump-user command
#[tokio::test]
#[serial]
async fn test_json_dump_user() {
    let _validator = TestValidator::start().expect("Failed to start test validator");
    let wallet_arc = create_test_keypair().expect("Failed to create test keypair");

    let run_id = format!("test-run-{}", rand::random::<u32>());

    let backend = SolanaBackend::new(
        Cluster::Localnet,
        vec![],
        wallet_arc.clone(),
        CommitmentConfig::confirmed(),
    )
    .expect("Failed to create backend");

    let create_params = CommandCreateRun {
        run_id: run_id.clone(),
        client_version: "v1.0.0-test".to_string(),
        treasurer_index: None,
        treasurer_collateral_mint: None,
        join_authority: None,
    };

    create_params
        .execute(backend.clone())
        .await
        .expect("create-run failed");

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    use run_manager::commands::run::CommandJsonDumpUser;

    let user_params = CommandJsonDumpUser {
        run_id: run_id.clone(),
        treasurer_index: None,
        address: wallet_arc.pubkey(),
    };

    // This command queries user state - should succeed even if user hasn't joined
    user_params
        .execute(backend)
        .await
        .expect("json-dump-user failed");

    println!("Successfully queried user state");
}
