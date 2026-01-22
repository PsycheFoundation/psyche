use psyche_coordinator::CommitteeSelection;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::SOLANA_MAX_NUM_WITNESSES;
use psyche_coordinator::WAITING_FOR_MEMBERS_EXTRA_SECONDS;
use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::model::HubRepo;
use psyche_coordinator::model::LLM;
use psyche_coordinator::model::Model;
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
use psyche_solana_tooling::process_coordinator_instructions::process_coordiantor_set_future_epoch_rates;
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
    let mut clients = vec![];
    for _ in 0..240 {
        clients.push(Keypair::new());
    }
    let ticker = Keypair::new();
    let warmup_time = 10;
    let round_witness_time = 10;
    let cooldown_time = 88;
    let epoch_time = 30;
    let earned_point_per_epoch_total_shared = 444_444;

    // Create the empty pre-allocated coordinator_account
    let coordinator_account = endpoint
        .process_system_new_exempt(
            &payer,
            CoordinatorAccount::space_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Initialize the coordinator
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

    // Prepare the configuration
    process_update(
        &mut endpoint,
        &payer,
        &main_authority,
        &coordinator_instance,
        &coordinator_account,
        None,
        Some(CoordinatorConfig {
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
            waiting_for_members_extra_time: WAITING_FOR_MEMBERS_EXTRA_SECONDS
                as u8,
            total_steps: 100,
        }),
        Some(Model::LLM(LLM {
            checkpoint: Checkpoint::Dummy(HubRepo::dummy()),
            max_seq_len: 4096,
            cold_start_warmup_steps: 0,
        })),
        None, // no explicit progress
    )
    .await
    .unwrap();

    // Set the reward rate for the epoch
    process_coordiantor_set_future_epoch_rates(
        &mut endpoint,
        &payer,
        &main_authority,
        &coordinator_instance,
        &coordinator_account,
        Some(earned_point_per_epoch_total_shared),
        None,
    )
    .await
    .unwrap();

    // Start the run
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

    // Add a few clients to the run
    for client in &clients {
        // Add clients to whitelist
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
        // Whitelisted, can join
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

    // Transition from warmup to round train
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

    // Run a full epoch
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
        // Tick from witness to train (or cooldown on the last round)
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

    // Tick from cooldown to next epoch's warmup (this should trigger reward distribution)
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

    // Check that all the rewards were distributed
    let coordinator_account_state =
        get_coordinator_account_state(&mut endpoint, &coordinator_account)
            .await
            .unwrap()
            .unwrap();
    for client in &clients {
        let client_state = coordinator_account_state
            .clients_state
            .clients
            .iter()
            .find(|c| c.id.signer.eq(&client.pubkey()))
            .unwrap();
        assert_eq!(
            client_state.earned,
            earned_point_per_epoch_total_shared / clients.len() as u64
        );
    }
}
