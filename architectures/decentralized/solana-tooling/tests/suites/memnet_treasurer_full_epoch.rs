use psyche_coordinator::CommitteeSelection;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::SOLANA_MAX_NUM_WITNESSES;
use psyche_coordinator::WAITING_FOR_MEMBERS_EXTRA_SECONDS;
use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::model::DummyType;
use psyche_coordinator::model::HubRepo;
use psyche_coordinator::model::LLM;
use psyche_coordinator::model::LLMArchitecture;
use psyche_coordinator::model::LLMTrainingDataLocation;
use psyche_coordinator::model::LLMTrainingDataType;
use psyche_coordinator::model::MAX_DATA_LOCATIONS;
use psyche_coordinator::model::Model;
use psyche_core::ConstantLR;
use psyche_core::FixedVec;
use psyche_core::LearningRateSchedule;
use psyche_core::OptimizerDefinition;
use psyche_solana_authorizer::logic::AuthorizationGranteeUpdateParams;
use psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams;
use psyche_solana_coordinator::ClientId;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_coordinator::instruction::Witness;
use psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::get_accounts::get_coordinator_account_state;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_create;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_grantee_update;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_grantor_update;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_join_run;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_tick;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_witness;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_participant_claim;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_participant_create;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_run_create;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_run_update;
use psyche_solana_treasurer::logic::RunCreateParams;
use psyche_solana_treasurer::logic::RunUpdateParams;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use std::vec;

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
    let main_authority = Keypair::new();
    let join_authority = Keypair::new();
    let participant = Keypair::new();
    let mut clients = vec![];
    for _ in 0..11 {
        clients.push(Keypair::new());
    }
    let ticker = Keypair::new();
    let minted_collateral_amount = 1_000_000_000_000_000;
    let top_up_collateral_amount = 0_999_999_999_999_999;
    let warmup_time = 10;
    let round_witness_time = 10;
    let cooldown_time = 42;
    let epoch_time = 30;
    let earned_point_per_epoch_total_shared = 888_888_888_888_888;
    let earned_point_per_epoch_per_client =
        earned_point_per_epoch_total_shared / clients.len() as u64;

    // Prepare the collateral mint
    let collateral_mint_authority = Keypair::new();
    let collateral_mint = endpoint
        .process_spl_token_mint_new(
            &payer,
            &collateral_mint_authority.pubkey(),
            None,
            6,
        )
        .await
        .unwrap();

    // Create the empty pre-allocated coordinator_account
    let coordinator_account = endpoint
        .process_system_new_exempt(
            &payer,
            CoordinatorAccount::space_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Create a run (it should create the underlying coordinator)
    let (run, coordinator_instance) = process_treasurer_run_create(
        &mut endpoint,
        &payer,
        &collateral_mint,
        &coordinator_account,
        RunCreateParams {
            index: 42,
            run_id: "This is my run's dummy run_id".to_string(),
            main_authority: main_authority.pubkey(),
            join_authority: join_authority.pubkey(),
            client_version: "latest".to_string(),
        },
    )
    .await
    .unwrap();

    // Get the run's collateral vault
    let run_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &run,
            &collateral_mint,
        )
        .await
        .unwrap();

    // Give the authority some collateral
    let main_authority_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &main_authority.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();
    endpoint
        .process_spl_token_mint_to(
            &payer,
            &collateral_mint,
            &collateral_mint_authority,
            &main_authority_collateral,
            minted_collateral_amount,
        )
        .await
        .unwrap();

    // Fund the run with some newly minted collateral
    endpoint
        .process_spl_token_transfer(
            &payer,
            &main_authority,
            &main_authority_collateral,
            &run_collateral,
            1,
        )
        .await
        .unwrap();

    // Create the clients ATAs
    let mut clients_collateral = vec![];
    for client in &clients {
        clients_collateral.push(
            endpoint
                .process_spl_associated_token_account_get_or_init(
                    &payer,
                    &client.pubkey(),
                    &collateral_mint,
                )
                .await
                .unwrap(),
        );
    }

    // Create the participations accounts
    for client in &clients {
        process_treasurer_participant_create(
            &mut endpoint,
            &payer,
            client,
            &run,
        )
        .await
        .unwrap();
    }

    // Try claiming nothing, it should work, but we earned nothing
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &clients[0],
        &clients_collateral[0],
        &collateral_mint,
        &run,
        &coordinator_account,
        0,
    )
    .await
    .unwrap();

    // Claiming with the wrong collateral should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &clients[0],
        &clients_collateral[1],
        &collateral_mint,
        &run,
        &coordinator_account,
        0,
    )
    .await
    .unwrap_err();

    // Prepare the coordinator's config
    process_treasurer_run_update(
        &mut endpoint,
        &payer,
        &main_authority,
        &run,
        &coordinator_instance,
        &coordinator_account,
        RunUpdateParams {
            metadata: None,
            config: Some(CoordinatorConfig {
                warmup_time,
                cooldown_time,
                max_round_train_time: 888,
                round_witness_time,
                min_clients: 1,
                init_min_clients: 1,
                global_batch_size_start: 1,
                global_batch_size_end: clients.len() as u16,
                global_batch_size_warmup_tokens: 0,
                verification_percent: 0,
                witness_nodes: 0,
                epoch_time,
                total_steps: 100,
                waiting_for_members_extra_time: 3,
            }),
            model: Some(Model::LLM(LLM {
                architecture: LLMArchitecture::HfLlama,
                checkpoint: Checkpoint::Dummy(HubRepo::dummy()),
                max_seq_len: 4096,
                data_type: LLMTrainingDataType::Pretraining,
                lr_schedule: LearningRateSchedule::Constant(
                    ConstantLR::default(),
                ),
                optimizer: OptimizerDefinition::Distro {
                    clip_grad_norm: None,
                    compression_decay: 1.0,
                    compression_topk: 1,
                    compression_chunk: 1,
                    quantize_1bit: false,
                    weight_decay: None,
                },
                cold_start_warmup_steps: 0,
            })),
            progress: None,
            epoch_earning_rate_total_shared: Some(
                earned_point_per_epoch_total_shared,
            ),
            epoch_slashing_rate_per_client: None,
            paused: Some(false),
            client_version: None,
            data_location: Some(LLMTrainingDataLocation::default()),
        },
    )
    .await
    .unwrap();

    // Add a participant key to whitelist
    let authorization = process_authorizer_authorization_create(
        &mut endpoint,
        &payer,
        &join_authority,
        &participant.pubkey(),
        JOIN_RUN_AUTHORIZATION_SCOPE,
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

    // Make the clients delegates of the participant key
    process_authorizer_authorization_grantee_update(
        &mut endpoint,
        &payer,
        &participant,
        &authorization,
        AuthorizationGranteeUpdateParams {
            delegates_clear: false,
            delegates_added: clients.iter().map(|c| c.pubkey()).collect(),
        },
    )
    .await
    .unwrap();

    // The clients can now join the run
    for client in &clients {
        process_coordinator_join_run(
            &mut endpoint,
            &payer,
            client,
            &authorization,
            &coordinator_instance,
            &coordinator_account,
            ClientId::new(client.pubkey(), Default::default()),
        )
        .await
        .unwrap();
    }

    // Tick to transition from waiting for members to warmup
    endpoint
        .forward_clock_unix_timestamp(WAITING_FOR_MEMBERS_EXTRA_SECONDS)
        .await
        .unwrap();
    process_coordinator_tick(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_instance,
        &coordinator_account,
    )
    .await
    .unwrap();

    // Tick from warmup to train
    endpoint
        .forward_clock_unix_timestamp(warmup_time)
        .await
        .unwrap();
    process_coordinator_tick(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_instance,
        &coordinator_account,
    )
    .await
    .unwrap();

    // Go through an epoch's rounds
    for _ in 0..4 {
        // Fetch the state at the start of the round
        let coordinator_account_state =
            get_coordinator_account_state(&mut endpoint, &coordinator_account)
                .await
                .unwrap()
                .unwrap();
        // Process clients round witness
        for client in &clients {
            let witness_proof = CommitteeSelection::from_coordinator(
                &coordinator_account_state.coordinator,
                0,
            )
            .unwrap()
            .get_witness(
                coordinator_account_state
                    .coordinator
                    .epoch_state
                    .clients
                    .iter()
                    .position(|c| c.id.signer.eq(&client.pubkey()))
                    .unwrap() as u64,
            );
            if witness_proof.position >= SOLANA_MAX_NUM_WITNESSES as u64 {
                continue;
            }
            process_coordinator_witness(
                &mut endpoint,
                &payer,
                client,
                &coordinator_instance,
                &coordinator_account,
                &Witness {
                    proof: witness_proof,
                    participant_bloom: Default::default(),
                    broadcast_bloom: Default::default(),
                    broadcast_merkle: Default::default(),
                    metadata: Default::default(),
                },
            )
            .await
            .unwrap();
        }
        // Tick from witness back next round train (or epoch cooldown after the last round)
        endpoint
            .forward_clock_unix_timestamp(round_witness_time)
            .await
            .unwrap();
        process_coordinator_tick(
            &mut endpoint,
            &payer,
            &ticker,
            &coordinator_instance,
            &coordinator_account,
        )
        .await
        .unwrap();
    }

    // Not yet earned the credit, claiming anything should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &clients[0],
        &clients_collateral[0],
        &collateral_mint,
        &coordinator_instance,
        &coordinator_account,
        1,
    )
    .await
    .unwrap_err();

    // Tick from cooldown to new epoch (should increment the earned points)
    endpoint
        .forward_clock_unix_timestamp(cooldown_time)
        .await
        .unwrap();
    process_coordinator_tick(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_instance,
        &coordinator_account,
    )
    .await
    .unwrap();

    // We can claim earned points now, but it should fail because run isnt funded
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &clients[0],
        &clients_collateral[0],
        &collateral_mint,
        &run,
        &coordinator_account,
        earned_point_per_epoch_per_client,
    )
    .await
    .unwrap_err();

    // We should be able to top-up run treasury at any time
    endpoint
        .process_spl_token_transfer(
            &payer,
            &main_authority,
            &main_authority_collateral,
            &run_collateral,
            top_up_collateral_amount,
        )
        .await
        .unwrap();

    // Now that a new epoch has started, we can claim our earned point
    for i in 0..clients.len() {
        let client = &clients[i];
        let client_collateral = &clients_collateral[i];
        process_treasurer_participant_claim(
            &mut endpoint,
            &payer,
            client,
            client_collateral,
            &collateral_mint,
            &run,
            &coordinator_account,
            earned_point_per_epoch_per_client,
        )
        .await
        .unwrap();
    }

    // Can't claim anything past the earned points
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &clients[0],
        &clients_collateral[0],
        &collateral_mint,
        &run,
        &coordinator_account,
        1,
    )
    .await
    .unwrap_err();

    // Check that we could claim only exactly the right amount
    for client_collateral in &clients_collateral {
        assert_eq!(
            endpoint
                .get_spl_token_account(client_collateral)
                .await
                .unwrap()
                .unwrap()
                .amount,
            earned_point_per_epoch_per_client,
        );
    }
    assert_eq!(
        endpoint
            .get_spl_token_account(&run_collateral)
            .await
            .unwrap()
            .unwrap()
            .amount,
        1 + top_up_collateral_amount
            - earned_point_per_epoch_per_client * clients.len() as u64,
    );
}
