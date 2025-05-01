use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::model::HubRepo;
use psyche_coordinator::model::LLMArchitecture;
use psyche_coordinator::model::LLMTrainingDataLocation;
use psyche_coordinator::model::LLMTrainingDataType;
use psyche_coordinator::model::Model;
use psyche_coordinator::model::LLM;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::RunState;
use psyche_coordinator::WitnessProof;
use psyche_core::ConstantLR;
use psyche_core::LearningRateSchedule;
use psyche_core::OptimizerDefinition;
use psyche_solana_authorizer::logic::AuthorizationGrantorUpdateParams;
use psyche_solana_coordinator::instruction::Witness;
use psyche_solana_coordinator::logic::InitCoordinatorParams;
use psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;
use psyche_solana_coordinator::ClientId;
use psyche_solana_coordinator::CoordinatorAccount;
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
        .process_airdrop(&payer.pubkey(), 10_000_000_000)
        .await
        .unwrap();

    // Run constants
    let main_authority = Keypair::new();
    let join_authority = Keypair::new();
    let client = Keypair::new();
    let ticker = Keypair::new();

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
            metadata: Default::default(),
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
            warmup_time: 1,
            cooldown_time: 1,
            max_round_train_time: 3,
            round_witness_time: 1,
            min_clients: 1,
            init_min_clients: 1,
            global_batch_size_start: 1,
            global_batch_size_end: 1,
            global_batch_size_warmup_tokens: 0,
            verification_percent: 0,
            witness_nodes: 1,
            rounds_per_epoch: 10,
            total_steps: 100,
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
    assert!(process_coordinator_join_run(
        &mut endpoint,
        &payer,
        &payer,
        &authorization,
        &coordinator_instance,
        &coordinator_account,
        client_id
    )
    .await
    .is_err());

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
    assert!(process_coordinator_tick(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_instance,
        &coordinator_account,
    )
    .await
    .is_err());

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

    // rejoin run
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

    // Pretend 5 second passed
    endpoint.forward_clock_unix_timestamp(5).await.unwrap();

    // Tick to transition from waiting for members to warmup
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

    // Pretend 1 second passed
    endpoint.forward_clock_unix_timestamp(1).await.unwrap();

    // tick should now succeed
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
    assert!(process_coordinator_witness(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_instance,
        &coordinator_account,
        &witness,
    )
    .await
    .is_err());
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
}
