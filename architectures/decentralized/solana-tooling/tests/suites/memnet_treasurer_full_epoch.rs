use bytemuck::Zeroable;
use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::model::LLMArchitecture;
use psyche_coordinator::model::LLMTrainingDataLocation;
use psyche_coordinator::model::LLMTrainingDataType;
use psyche_coordinator::model::Model;
use psyche_coordinator::model::LLM;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::WitnessProof;
use psyche_coordinator::SOLANA_MAX_STRING_LEN;
use psyche_core::ConstantLR;
use psyche_core::FixedVec;
use psyche_core::LearningRateSchedule;
use psyche_core::OptimizerDefinition;
use psyche_solana_coordinator::instruction::Witness;
use psyche_solana_coordinator::logic::JOIN_RUN_AUTHORIZATION_SCOPE;
use psyche_solana_coordinator::ClientId;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_create;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_grantee_update;
use psyche_solana_tooling::process_authorizer_instructions::process_authorizer_authorization_grantor_update;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_join_run;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_tick;
use psyche_solana_tooling::process_coordinator_instructions::process_coordinator_witness;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_participant_claim;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_participant_create;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_run_create;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_run_top_up;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_run_update;
use psyche_solana_treasurer::logic::RunCreateParams;
use psyche_solana_treasurer::logic::RunUpdateParams;
use solana_sdk::pubkey::Pubkey;
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

    // Constants
    let main_authority = Keypair::new();
    let join_authority = Keypair::new();
    let participant = Keypair::new();
    let client = Keypair::new();
    let ticker = Keypair::new();
    let earned_point_per_epoch = 33;
    let collateral_amount_per_earned_point = 42;

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

    // create the empty pre-allocated coordinator_account
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
            run_id: "This is my run's dummy run_id".to_string(),
            main_authority: main_authority.pubkey(),
            join_authority: join_authority.pubkey(),
            collateral_amount_per_earned_point,
            metadata: Default::default(),
        },
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
            10_000_000,
        )
        .await
        .unwrap();

    // Fund the run with some newly minted collateral
    process_treasurer_run_top_up(
        &mut endpoint,
        &payer,
        &main_authority,
        &main_authority_collateral,
        &collateral_mint,
        &run,
        5_000_000,
    )
    .await
    .unwrap();

    // Create the client ATA
    let client_collateral = endpoint
        .process_spl_associated_token_account_get_or_init(
            &payer,
            &client.pubkey(),
            &collateral_mint,
        )
        .await
        .unwrap();

    // Create the participation account
    process_treasurer_participant_create(&mut endpoint, &payer, &client, &run)
        .await
        .unwrap();

    // Try claiming nothing, it should work since we earned nothing
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client,
        &client_collateral,
        &collateral_mint,
        &run,
        &coordinator_account,
        0,
    )
    .await
    .unwrap();

    // Claiming something while we havent earned anything should fail
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client,
        &client_collateral,
        &collateral_mint,
        &run,
        &coordinator_account,
        1,
    )
    .await
    .unwrap_err();

    // We should be able to top-up run treasury at any time
    process_treasurer_run_top_up(
        &mut endpoint,
        &payer,
        &main_authority,
        &main_authority_collateral,
        &collateral_mint,
        &run,
        5_000_000,
    )
    .await
    .unwrap();

    // Prepare the coordinator's config
    process_treasurer_run_update(
        &mut endpoint,
        &payer,
        &main_authority,
        &run,
        &coordinator_instance,
        &coordinator_account,
        RunUpdateParams {
            config: Some(CoordinatorConfig::<ClientId> {
                warmup_time: 1,
                cooldown_time: 1,
                max_round_train_time: 3,
                round_witness_time: 1,
                min_clients: 1,
                global_batch_size: 1,
                verification_percent: 0,
                witness_nodes: 1,
                rounds_per_epoch: 4,
                total_steps: 100,
                checkpointers: FixedVec::zeroed(),
            }),
            model: Some(Model::LLM(LLM {
                architecture: LLMArchitecture::HfLlama,
                checkpoint: Checkpoint::Ephemeral,
                max_seq_len: 4096,
                data_type: LLMTrainingDataType::Pretraining,
                data_location: LLMTrainingDataLocation::Local(
                    Zeroable::zeroed(),
                ),
                lr_schedule: LearningRateSchedule::Constant(
                    ConstantLR::default(),
                ),
                optimizer: OptimizerDefinition::Distro {
                    clip_grad_norm: None,
                    compression_decay: 1.0,
                    compression_decay_warmup_steps: 0,
                    compression_topk: 1,
                    compression_topk_startup: 0,
                    compression_topk_startup_steps: 0,
                    compression_chunk: 1,
                    quantize_1bit: false,
                    weight_decay: None,
                },
            })),
            epoch_earning_rate: Some(earned_point_per_epoch),
            epoch_slashing_rate: None,
            paused: Some(false),
        },
    )
    .await
    .unwrap();

    // Generate the client key
    let client_id = ClientId::new(client.pubkey(), Default::default());

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
        true,
    )
    .await
    .unwrap();

    // Make the client a delegate of the participant key
    process_authorizer_authorization_grantee_update(
        &mut endpoint,
        &payer,
        &participant,
        &authorization,
        &[Pubkey::new_unique(), client.pubkey()],
    )
    .await
    .unwrap();

    // The client can now join the run
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

    // Tick to witness
    endpoint.forward_clock_unix_timestamp(10).await.unwrap();
    process_coordinator_tick(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_instance,
        &coordinator_account,
    )
    .await
    .unwrap();

    for _ in 0..4 {
        // Witness
        process_coordinator_witness(
            &mut endpoint,
            &payer,
            &client,
            &coordinator_instance,
            &coordinator_account,
            &Witness {
                proof: WitnessProof {
                    witness: true.into(),
                    position: 0,
                    index: 0,
                },
                participant_bloom: Default::default(),
                broadcast_bloom: Default::default(),
                broadcast_merkle: Default::default(),
            },
        )
        .await
        .unwrap();

        // Tick from witness to train (or cooldown on the last one)
        endpoint.forward_clock_unix_timestamp(2).await.unwrap();
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
        &client,
        &client_collateral,
        &collateral_mint,
        &coordinator_instance,
        &coordinator_account,
        1,
    )
    .await
    .unwrap_err();

    // Tick from cooldown to new epoch (should increment the earned)
    endpoint.forward_clock_unix_timestamp(10).await.unwrap();
    process_coordinator_tick(
        &mut endpoint,
        &payer,
        &ticker,
        &coordinator_instance,
        &coordinator_account,
    )
    .await
    .unwrap();

    // Now that a new epoch has started, we can claim our earned point
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client,
        &client_collateral,
        &collateral_mint,
        &run,
        &coordinator_account,
        earned_point_per_epoch,
    )
    .await
    .unwrap();

    // Can't claim anything past the earned points
    process_treasurer_participant_claim(
        &mut endpoint,
        &payer,
        &client,
        &client_collateral,
        &collateral_mint,
        &run,
        &coordinator_account,
        1,
    )
    .await
    .unwrap_err();
}
