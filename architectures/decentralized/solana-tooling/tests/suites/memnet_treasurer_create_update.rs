use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::WAITING_FOR_MEMBERS_EXTRA_SECONDS;
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
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_tooling::create_memnet_endpoint::create_memnet_endpoint;
use psyche_solana_tooling::process_treasurer_instructions::process_treasurer_run_create;
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
        .request_airdrop(&payer.pubkey(), 5_000_000_000)
        .await
        .unwrap();

    // Constants
    let main_authority = Keypair::new();
    let index = 42;
    let run_id = "This is my run's dummy run_id".to_string();
    let run_update_params = RunUpdateParams {
        metadata: None,
        config: Some(CoordinatorConfig {
            warmup_time: 10,
            cooldown_time: 88,
            max_round_train_time: 15,
            round_witness_time: 10,
            min_clients: 1,
            init_min_clients: 1,
            global_batch_size_start: 1,
            global_batch_size_end: 1,
            global_batch_size_warmup_tokens: 0,
            verification_percent: 0,
            witness_nodes: 1,
            epoch_time: 30,
            total_steps: 100,
            waiting_for_members_extra_time: WAITING_FOR_MEMBERS_EXTRA_SECONDS
                as u8,
        }),
        model: Some(Model::LLM(LLM {
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
        progress: None,
        epoch_earning_rate_total_shared: Some(66),
        epoch_slashing_rate_per_client: None,
        paused: Some(false),
        client_version: None,
    };

    // Prepare the collateral mint
    let collateral_mint = endpoint
        .process_spl_token_mint_new(&payer, &Pubkey::new_unique(), None, 6)
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

    // Create a run should fail without a proper coordinator account
    process_treasurer_run_create(
        &mut endpoint,
        &payer,
        &collateral_mint,
        &Pubkey::new_unique(),
        RunCreateParams {
            index,
            run_id: run_id.clone(),
            main_authority: main_authority.pubkey(),
            join_authority: Pubkey::new_unique(),
            client_version: "latest".to_string(),
        },
    )
    .await
    .unwrap_err();

    // Create a run should fail without a proper collateral mint
    process_treasurer_run_create(
        &mut endpoint,
        &payer,
        &Pubkey::new_unique(),
        &coordinator_account,
        RunCreateParams {
            index,
            run_id: run_id.clone(),
            main_authority: main_authority.pubkey(),
            join_authority: Pubkey::new_unique(),
            client_version: "latest".to_string(),
        },
    )
    .await
    .unwrap_err();

    // Create a run with correct inputs, should succeed now
    let (run, coordinator_instance) = process_treasurer_run_create(
        &mut endpoint,
        &payer,
        &collateral_mint,
        &coordinator_account,
        RunCreateParams {
            index,
            run_id: run_id.clone(),
            main_authority: main_authority.pubkey(),
            join_authority: Pubkey::new_unique(),
            client_version: "latest".to_string(),
        },
    )
    .await
    .unwrap();

    // Update the run's configuration with the wrong authority should fail
    process_treasurer_run_update(
        &mut endpoint,
        &payer,
        &Keypair::new(),
        &run,
        &coordinator_instance,
        &coordinator_account,
        run_update_params.clone(),
    )
    .await
    .unwrap_err();

    // Update the run's configuration with the correct authority should succeed
    process_treasurer_run_update(
        &mut endpoint,
        &payer,
        &main_authority,
        &run,
        &coordinator_instance,
        &coordinator_account,
        run_update_params.clone(),
    )
    .await
    .unwrap();

    // Create another run with the same coordinator account should fail
    process_treasurer_run_create(
        &mut endpoint,
        &payer,
        &collateral_mint,
        &coordinator_account,
        RunCreateParams {
            index: index + 1,
            run_id: "another run id".to_string(),
            main_authority: main_authority.pubkey(),
            join_authority: Pubkey::new_unique(),
            client_version: "latest".to_string(),
        },
    )
    .await
    .unwrap_err();

    // Prepare a second dummy coordinator account
    let coordinator_account2 = endpoint
        .process_system_new_exempt(
            &payer,
            CoordinatorAccount::space_with_discriminator(),
            &psyche_solana_coordinator::ID,
        )
        .await
        .unwrap();

    // Create a run with a new coordinator account but the same index should fail
    process_treasurer_run_create(
        &mut endpoint,
        &payer,
        &collateral_mint,
        &coordinator_account2,
        RunCreateParams {
            index,
            run_id: "another run id".to_string(),
            main_authority: main_authority.pubkey(),
            join_authority: Pubkey::new_unique(),
            client_version: "latest".to_string(),
        },
    )
    .await
    .unwrap_err();

    // Create a run with a new coordinator account but the same run_id should fail
    process_treasurer_run_create(
        &mut endpoint,
        &payer,
        &collateral_mint,
        &coordinator_account2,
        RunCreateParams {
            index: index + 1,
            run_id: run_id.clone(),
            main_authority: main_authority.pubkey(),
            join_authority: Pubkey::new_unique(),
            client_version: "latest".to_string(),
        },
    )
    .await
    .unwrap_err();

    // Creating a completely separate run should succeed
    process_treasurer_run_create(
        &mut endpoint,
        &payer,
        &collateral_mint,
        &coordinator_account2,
        RunCreateParams {
            index: index + 1,
            run_id: "another run id".to_string(),
            main_authority: main_authority.pubkey(),
            join_authority: Pubkey::new_unique(),
            client_version: "latest".to_string(),
        },
    )
    .await
    .unwrap();
}
