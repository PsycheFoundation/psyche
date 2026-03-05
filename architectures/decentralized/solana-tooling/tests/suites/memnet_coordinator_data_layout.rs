use psyche_coordinator::ClientState;
use psyche_coordinator::Coordinator;
use psyche_coordinator::CoordinatorConfig;
use psyche_coordinator::CoordinatorEpochState;
use psyche_coordinator::CoordinatorProgress;
use psyche_coordinator::Round;
use psyche_coordinator::RunState;
use psyche_coordinator::Witness;
use psyche_coordinator::WitnessProof;
use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::model::HttpLLMTrainingDataLocation;
use psyche_coordinator::model::HttpTrainingDataLocation;
use psyche_coordinator::model::HubRepo;
use psyche_coordinator::model::LLM;
use psyche_coordinator::model::LLMArchitecture;
use psyche_coordinator::model::LLMTrainingDataLocation;
use psyche_coordinator::model::LLMTrainingDataType;
use psyche_coordinator::model::Model;
use psyche_core::Bloom;
use psyche_core::CosineLR;
use psyche_core::FixedString;
use psyche_core::FixedVec;
use psyche_core::LearningRateSchedule;
use psyche_core::MerkleRoot;
use psyche_core::NodeIdentity;
use psyche_core::OptimizerDefinition;
use psyche_core::Shuffle;
use psyche_core::SmallBoolean;
use psyche_core::TokenSize;
use psyche_solana_coordinator::ClientsEpochRates;
use psyche_solana_coordinator::ClientsState;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_coordinator::CoordinatorInstanceState;
use psyche_solana_coordinator::RunMetadata;
use psyche_solana_coordinator::coordinator_account_from_bytes;
use solana_sdk::pubkey::Pubkey;

#[tokio::test]
pub async fn run() {
    let coordinator_account_struct = CoordinatorAccount {
        version: CoordinatorAccount::VERSION,
        state: CoordinatorInstanceState {
            metadata: RunMetadata {
                name: fixed_str("my-name"),
                description: fixed_str("my-description"),
                num_parameters: 1100000000,
                vocab_size: 4242_32768,
            },
            coordinator: Coordinator {
                run_id: fixed_str("my-run-id"),
                run_state: RunState::RoundTrain,
                model: Model::LLM(LLM {
                    max_seq_len: 2048,
                    cold_start_warmup_steps: 999,
                    architecture: LLMArchitecture::HfAuto,
                    checkpoint: Checkpoint::Hub(HubRepo {
                        repo_id: fixed_str("my-repo-id"),
                        revision: Some(fixed_str("my-revision")),
                    }),
                    data_type: LLMTrainingDataType::Finetuning,
                    data_location: LLMTrainingDataLocation::Http(
                        HttpLLMTrainingDataLocation {
                            location: HttpTrainingDataLocation::Gcp {
                                bucket_name: fixed_str("my-bucket-name"),
                                filter_directory: fixed_str(
                                    "my-filter-directory",
                                ),
                            },
                            token_size_in_bytes: TokenSize::FourBytes,
                            shuffle: Shuffle::Seeded([55; 32]),
                        },
                    ),
                    lr_schedule: LearningRateSchedule::Cosine(CosineLR::new(
                        0.0004, 250, 0.666, 25000, 0.00004,
                    )),
                    optimizer: OptimizerDefinition::Distro {
                        clip_grad_norm: Some(1.0),
                        weight_decay: Some(42.42),
                        compression_decay: 0.999,
                        compression_topk: 2,
                        compression_chunk: 64,
                        quantize_1bit: true,
                    },
                }),
                config: CoordinatorConfig {
                    warmup_time: 15,
                    cooldown_time: 30,
                    max_round_train_time: 15,
                    round_witness_time: 1,
                    global_batch_size_warmup_tokens: 34,
                    epoch_time: 60,
                    total_steps: 25000,
                    init_min_clients: 1,
                    min_clients: 1,
                    witness_nodes: 88,
                    global_batch_size_start: 2048,
                    global_batch_size_end: 2048,
                    verification_percent: 42,
                    waiting_for_members_extra_time: 3,
                },
                progress: CoordinatorProgress {
                    epoch: 8989,
                    step: 777,
                    epoch_start_data_index: 574842891,
                },
                epoch_state: CoordinatorEpochState {
                    rounds: [Round {
                        witnesses: fixed_vec_repeat(Witness {
                            proof: WitnessProof {
                                position: 42,
                                index: 32,
                                witness: SmallBoolean::TRUE,
                            },
                            participant_bloom: Bloom::new(4, &[7; 8]),
                            broadcast_bloom: Bloom::new(4, &[6; 8]),
                            broadcast_merkle: MerkleRoot { inner: [77; 32] },
                        }),
                        data_index: 893322,
                        random_seed: 871,
                        height: 1002,
                        clients_len: 21,
                        tie_breaker_tasks: 34,
                    }; 4],
                    clients: fixed_vec_repeat(psyche_coordinator::Client {
                        id: NodeIdentity::from_single_key([77; 32]),
                        state: ClientState::Dropped,
                        exited_height: 42,
                    }),
                    exited_clients: fixed_vec_repeat(
                        psyche_coordinator::Client {
                            id: NodeIdentity::from_single_key([99; 32]),
                            state: ClientState::Dropped,
                            exited_height: 48,
                        },
                    ),
                    rounds_head: 77,
                    start_step: 88,
                    last_step: 99,
                    start_timestamp: 33,
                    first_round: SmallBoolean::TRUE,
                    cold_start_epoch: SmallBoolean::TRUE,
                },
                pending_pause: SmallBoolean::TRUE,
                run_state_start_unix_timestamp: 55_55_555_555,
            },
            clients_state: ClientsState {
                clients: fixed_vec_repeat(psyche_solana_coordinator::Client {
                    id: NodeIdentity::from_single_key([33; 32]),
                    active: 63473857845,
                    earned: 424242,
                    slashed: 7878,
                    claimer: Pubkey::from([88; 32]),
                }),
                next_active: 63473857845,
                current_epoch_rates: ClientsEpochRates {
                    earning_rate_total_shared: 727272,
                    slashing_rate_per_client: 7272,
                },
                future_epoch_rates: ClientsEpochRates {
                    earning_rate_total_shared: 424242,
                    slashing_rate_per_client: 4242,
                },
            },
            is_warmup_first_tick: SmallBoolean::TRUE,
            is_training_first_tick: SmallBoolean::TRUE,
            client_version: fixed_str("my-client-version"),
        },
        nonce: 78787878,
    };
    let coordinator_account_bytes_from_struct =
        bytemuck::bytes_of(&coordinator_account_struct);
    let coordinator_account_bytes_from_snapshot =
        include_bytes!("../fixtures/coordinator-account.so");
    /*
    std::fs::write(
        "./tests/fixtures/coordinator-account.so",
        coordinator_account_bytes_from_struct,
    )
    .unwrap();
    */
    assert_eq!(
        coordinator_account_bytes_from_struct,
        coordinator_account_bytes_from_snapshot
    );
}

fn fixed_str<const L: usize>(value: &str) -> FixedString<L> {
    FixedString::from_str_truncated(value)
}

fn fixed_vec_repeat<const N: usize, T: Default + Copy>(
    value: T,
) -> FixedVec<T, N> {
    let mut vec = FixedVec::new();
    for _ in 0..N {
        vec.push(value).unwrap();
    }
    vec
}
