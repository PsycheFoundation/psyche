use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::RunState;
use psyche_coordinator::WAITING_FOR_MEMBERS_EXTRA_SECONDS;
use psyche_coordinator::WitnessProof;
use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::model::HubRepo;
use psyche_coordinator::model::LLM;
use psyche_coordinator::model::LLMArchitecture;
use psyche_coordinator::model::LLMTrainingDataLocation;
use psyche_coordinator::model::LLMTrainingDataType;
use psyche_coordinator::model::Model;
use psyche_core::ConstantLR;
use psyche_core::LearningRateSchedule;
use psyche_core::OptimizerDefinition;
use psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams;
use psyche_solana_coordinator::ClientId;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_coordinator::instruction::Witness;
use psyche_solana_coordinator::logic::InitCoordinatorParams;
use psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::get_accounts::get_coordinator_account_state;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_create;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_grantor_update;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_init;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_join_run;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_set_paused;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_tick;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_witness;
use psyche_solana_tooling::process_coordinator_instructions::process_update;
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
    let client = Keypair::new();
    let ticker = Keypair::new();
    let warmup_time = 77;
    let round_witness_time = 88;

    // create the empty pre-allocated coordinator_account
    let coordinator_account = endpoint
        .process_system_new_exempt(
            &payer,
            CoordinatorAccount::space_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // initialize the coordinator
    let coordinator_instance = process_coordinator_init(
        &mut endpoint,
        &payer,
        &coordinator_account,
        InitCoordinatorParams {
            run_id: "This is a random run id!".to_string(),
            main_authority: main_authority.pubkey(),
            join_authority: join_authority.pubkey(),
            client_version: "test".to_string(),
        },
    )
    .await
    .unwrap();

    // verify that the run is in initialized state
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    // update the coordinator's model
    process_update(
        &mut endpoint,
        &payer,
        &main_authority,
        &coordinator_instance,
        &coordinator_account,
        None,
        Some(CoordinatorConfig {
            warmup_time,
            cooldown_time: 999,
            max_round_train_time: 888,
            round_witness_time,
            min_clients: 1,
            init_min_clients: 1,
            global_batch_size_start: 1,
            global_batch_size_end: 1,
            global_batch_size_warmup_tokens: 0,
            verification_percent: 0,
            witness_nodes: 1,
            epoch_time: 30,
            total_steps: 100,
            waiting_for_members_extra_time: 3,
        }),
        Some(Model::LLM(LLM {
            architecture: LLMArchitecture::HfLlama,
            checkpoint: Checkpoint::Dummy(HubRepo::dummy()),
            max_seq_len: 4096,
            data_type: LLMTrainingDataType::Pretraining,
            data_location: LLMTrainingDataLocation::default(),
            lr_schedule: LearningRateSchedule::Constant(ConstantLR::default()),
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
        None, // no explicit progress
    )
    .await
    .unwrap();

    // Coordinator's state should now have changed
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    // Can't tick yet because paused/uninitialized
    assert!(
        process_coordinator_tick(
            &mut endpoint,
            &payer,
            &ticker,
            &coordinator_instance,
            &coordinator_account,
        )
        .await
        .is_err()
    );

    // Generate the client key
    let client_id = ClientId::new(client.pubkey(), Default::default());

    // Add client to whitelist
    let authorization = process_authorizer_authorization_create(
        &mut endpoint,
        &payer,
        &join_authority,
        &client.pubkey(),
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

    // Whitelisted with the wrong account, can't join
    process_coordinator_join_run(
        &mut endpoint,
        &payer,
        &payer,
        &authorization,
        &coordinator_instance,
        &coordinator_account,
        client_id,
    )
    .await
    .unwrap_err();

    // Whitelisted, can join
    process_coordinator_join_run(
        &mut endpoint,
        &payer,
        &client,
        &authorization,
        &coordinator_instance,
        &coordinator_account,
        client_id,
    )
    .await
    .unwrap();

    // Coordinator should still not be ready
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .unwrap()
            .coordinator
            .run_state,
        RunState::Uninitialized
    );

    // Can't tick yet because paused
    process_coordinator_tick(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_instance,
        &coordinator_account,
    )
    .await
    .unwrap_err();

    // Unpause
    process_coordinator_set_paused(
        &mut endpoint,
        &payer,
        &main_authority,
        &coordinator_instance,
        &coordinator_account,
        false,
    )
    .await
    .unwrap();

    // Rejoin run after waiting a while, should be a no-op
    endpoint.forward_clock_slot(1).await.unwrap();
    process_coordinator_join_run(
        &mut endpoint,
        &payer,
        &client,
        &authorization,
        &coordinator_instance,
        &coordinator_account,
        client_id,
    )
    .await
    .unwrap();

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

    // Coordinator should have changed
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .unwrap()
            .coordinator
            .run_state,
        RunState::Warmup
    );

    // Tick from warmup to round train
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

    // Coordinator in train mode
    let coordinator =
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .unwrap()
            .coordinator;
    assert_eq!(coordinator.run_state, RunState::RoundTrain);
    assert_eq!(coordinator.current_round().unwrap().height, 0);
    assert_eq!(coordinator.progress.step, 1);

    // Check that only the right user can successfully send a witness
    let witness = Witness {
        proof: WitnessProof {
            witness: true.into(),
            position: 0,
            index: 0,
        },
        participant_bloom: Default::default(),
        broadcast_bloom: Default::default(),
        broadcast_merkle: Default::default(),
        metadata: Default::default(),
    };
    process_coordinator_witness(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_instance,
        &coordinator_account,
        &witness,
    )
    .await
    .unwrap_err();
    process_coordinator_witness(
        &mut endpoint,
        &payer,
        &client,
        &coordinator_instance,
        &coordinator_account,
        &witness,
    )
    .await
    .unwrap();

    // Coordinator state after witness should change
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .unwrap()
            .coordinator
            .run_state,
        RunState::RoundWitness
    );

    // Tick from round witness back to round train should work
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

    // Coordinator state after witness should change
    assert_eq!(
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .unwrap()
            .coordinator
            .run_state,
        RunState::RoundTrain
    );
}
