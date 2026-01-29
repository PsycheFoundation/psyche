use psyche_coordinator::Round;
use psyche_coordinator::RunState;
use psyche_coordinator::model::Checkpoint;
use psyche_coordinator::model::Model;
use psyche_core::FixedString;
use psyche_core::FixedVec;
use psyche_core::SmallBoolean;
use psyche_solana_coordinator::CoordinatorAccount;
use psyche_solana_coordinator::coordinator_account_from_bytes;

#[tokio::test]
pub async fn run() {
    let coordinator_bytes =
        include_bytes!("../fixtures/coordinator-account-v1.so").to_vec();
    let coordinator_account =
        coordinator_account_from_bytes(&coordinator_bytes).unwrap();
    eprintln!("coordinator_account.state:{:#?}", coordinator_account.state);
    // Check the general layout for corruption
    assert_eq!(coordinator_account.version, CoordinatorAccount::VERSION);
    assert_eq!(coordinator_account.nonce, 2);
    let state = coordinator_account.state;
    assert_eq!(state.is_warmup_first_tick, SmallBoolean::FALSE);
    assert_eq!(state.is_training_first_tick, SmallBoolean::FALSE);
    assert_eq!(state.client_version, fixed_str("test"));
    // Check infos on the coordinator run metadata
    let metadata = state.metadata;
    assert_eq!(metadata.name, fixed_str(""));
    assert_eq!(metadata.description, fixed_str(""));
    assert_eq!(metadata.num_parameters, 1100000000);
    assert_eq!(metadata.vocab_size, 32768);
    // Check on the on the coordinator datastructure
    let coordinator = state.coordinator;
    assert_eq!(coordinator.run_id, fixed_str("test"));
    assert_eq!(coordinator.run_state, RunState::Uninitialized);
    assert_eq!(coordinator.run_state_start_unix_timestamp, 0);
    assert_eq!(coordinator.pending_pause, SmallBoolean::FALSE);
    // Coordinator model (only on-chain fields)
    match coordinator.model {
        Model::LLM(llm) => {
            assert_eq!(llm.max_seq_len, 2048);
            assert_eq!(llm.cold_start_warmup_steps, 0);
            match llm.checkpoint {
                Checkpoint::Hub(hub) => {
                    assert_eq!(
                        hub.repo_id,
                        fixed_str("emozilla/llama2-1.1b-gqa-init")
                    );
                    assert_eq!(hub.revision, None);
                },
                _ => panic!("Expected Hub checkpoint"),
            };
        },
    };
    // Coordinator config
    assert_eq!(coordinator.config.warmup_time, 15);
    assert_eq!(coordinator.config.cooldown_time, 30);
    assert_eq!(coordinator.config.max_round_train_time, 15);
    assert_eq!(coordinator.config.round_witness_time, 1);
    assert_eq!(coordinator.config.global_batch_size_warmup_tokens, 0);
    assert_eq!(coordinator.config.epoch_time, 60);
    assert_eq!(coordinator.config.total_steps, 25000);
    assert_eq!(coordinator.config.init_min_clients, 1);
    assert_eq!(coordinator.config.min_clients, 1);
    assert_eq!(coordinator.config.witness_nodes, 0);
    assert_eq!(coordinator.config.global_batch_size_start, 2048);
    assert_eq!(coordinator.config.global_batch_size_end, 2048);
    assert_eq!(coordinator.config.verification_percent, 0);
    assert_eq!(coordinator.config.waiting_for_members_extra_time, 3);
    // Coordinator progress
    assert_eq!(coordinator.progress.epoch, 0);
    assert_eq!(coordinator.progress.step, 0);
    assert_eq!(coordinator.progress.epoch_start_data_index, 0);
    // Coordinator epoch state
    let epoch_state = coordinator.epoch_state;
    assert_eq!(epoch_state.rounds, [Round::default(); 4]);
    assert_eq!(epoch_state.clients, FixedVec::default());
    assert_eq!(epoch_state.exited_clients, FixedVec::default());
    assert_eq!(epoch_state.rounds_head, 0);
    assert_eq!(epoch_state.start_step, 0);
    assert_eq!(epoch_state.last_step, 0);
    assert_eq!(epoch_state.start_timestamp, 0);
    assert_eq!(epoch_state.first_round, SmallBoolean::FALSE);
    assert_eq!(epoch_state.cold_start_epoch, SmallBoolean::FALSE);
    // Coordinator clients state
    let clients_state = state.clients_state;
    assert_eq!(clients_state.clients.len(), 0);
    assert_eq!(clients_state.next_active, 0);
    let current_epoch_rates = clients_state.current_epoch_rates;
    assert_eq!(current_epoch_rates.earning_rate_total_shared, 0);
    assert_eq!(current_epoch_rates.slashing_rate_per_client, 0);
    let future_epoch_rates = clients_state.future_epoch_rates;
    assert_eq!(future_epoch_rates.earning_rate_total_shared, 1000000);
    assert_eq!(future_epoch_rates.slashing_rate_per_client, 0);
}

fn fixed_str<const L: usize>(value: &str) -> FixedString<L> {
    FixedString::from_str_truncated(value)
}
