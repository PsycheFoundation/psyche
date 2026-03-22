#![allow(unexpected_cfgs)]

mod batch_id;
mod bounded_queue;
mod boxed_future;
mod cancellable_barrier;
mod commitment;
mod coordinator;
mod data_selection;
mod data_shuffle;
mod definitions;
mod deterministic_shuffle;
mod interval_tree;
mod lcg;
mod merkle_tree;
mod model_extra_data;
mod running_average;
mod serde_utils;
mod similarity;
mod sized_iterator;
mod testing;
mod token_size;

pub use coordinator::{
    HttpLLMTrainingDataLocation, HttpTrainingDataLocation, LLMArchitecture,
    LLMTrainingDataLocation, LLMTrainingDataLocationAndWeight, LLMTrainingDataType,
    WitnessEvalResult, WitnessMetadata,
};

pub use commitment::{Commitment, select_consensus_commitment_by_witnesses};
pub use data_selection::{
    assign_data_for_state, get_batch_ids_for_node, get_batch_ids_for_round, get_data_index_for_step,
};

pub use batch_id::BatchId;
pub use bounded_queue::BoundedQueue;
pub use boxed_future::BoxedFuture;
pub use cancellable_barrier::{Barrier, CancellableBarrier, CancelledBarrier};
pub use data_shuffle::Shuffle;
pub use definitions::{
    ConstantLR, CosineLR, LearningRateSchedule, LearningRateScheduler, LinearLR,
    OptimizerDefinition,
};
pub use deterministic_shuffle::deterministic_shuffle;
pub use interval_tree::{ClosedInterval, IntervalTree};
pub use lcg::LCG;
pub use merkle_tree::{MerkleTree, OwnedProof, Proof};
pub use model_extra_data::{
    CHECKPOINT_DATA_MAX_LEN, CONFIG_PREFIX, CheckpointData, MODEL_CONFIG_FILENAME, ModelExtraData,
    RunMetadata,
};
pub use running_average::RunningAverage;
pub use serde_utils::{serde_deserialize_vec_to_array, serde_serialize_array_as_vec};
pub use similarity::{
    DistanceThresholds, hamming_distance, is_similar, jaccard_distance, manhattan_distance,
};
pub use sized_iterator::SizedIterator;
pub use testing::IntegrationTestLogMarker;
pub use token_size::TokenSize;

#[cfg(test)]
mod tests {
    /// A lot of the code here assumes that usize is u64. This should be true on every platform we support.
    #[test]
    fn test_check_type_assumptions() {
        assert_eq!(size_of::<u64>(), size_of::<usize>());
    }
}
