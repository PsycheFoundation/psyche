#![allow(unexpected_cfgs)]

mod commitment;
mod committee_selection;
mod coordinator;
mod data_selection;
pub mod model;

pub use commitment::Commitment;
pub use committee_selection::{
    Committee, CommitteeProof, CommitteeSelection, WitnessProof, COMMITTEE_SALT, WITNESS_SALT,
};
pub use coordinator::{
    Client, ClientState, Coordinator, CoordinatorConfig, CoordinatorEpochState, CoordinatorError,
    CoordinatorProgress, HealthChecks, Round, RunState, TickResult, Witness, WitnessBloom,
    WitnessEvalResult, WitnessMetadata, BLOOM_FALSE_RATE, NUM_STORED_ROUNDS,
    SOLANA_MAX_NUM_CLIENTS, SOLANA_MAX_NUM_WITNESSES, SOLANA_MAX_STRING_LEN,
};
pub use data_selection::{
    assign_data_for_state, get_batch_ids_for_node, get_batch_ids_for_round, get_data_index_for_step,
};
