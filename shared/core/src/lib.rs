#![allow(unexpected_cfgs)]

mod batch_id;
mod bloom;
mod bounded_queue;
mod boxed_future;
mod cancellable_barrier;
mod data_shuffle;
mod definitions;
mod deterministic_shuffle;
mod fixed_string;
mod fixed_vec;
mod interval_tree;
mod lcg;
mod merkle_tree;
mod node_identity;
mod running_average;
mod serde_utils;
mod sha256;
mod similarity;
mod sized_iterator;
mod small_boolean;
mod swap_or_not;
mod testing;
mod token_size;

pub use batch_id::BatchId;
pub use bloom::Bloom;
pub use bounded_queue::BoundedQueue;
pub use boxed_future::BoxedFuture;
pub use cancellable_barrier::{Barrier, CancellableBarrier, CancelledBarrier};
pub use data_shuffle::Shuffle;
pub use definitions::{
    ConstantLR, CosineLR, LearningRateSchedule, LearningRateScheduler, LinearLR,
    OptimizerDefinition,
};
pub use deterministic_shuffle::deterministic_shuffle;
pub use fixed_string::FixedString;
pub use fixed_vec::FixedVec;
pub use interval_tree::{ClosedInterval, IntervalTree};
pub use lcg::LCG;
pub use merkle_tree::{HashWrapper as MerkleRoot, MerkleTree, OwnedProof, Proof};
pub use node_identity::NodeIdentity;
pub use running_average::RunningAverage;
pub use serde_utils::{
    serde_deserialize_optional_string, serde_deserialize_string, serde_deserialize_vec_to_array,
    serde_serialize_array_as_vec, serde_serialize_optional_string, serde_serialize_string,
};
pub use sha256::{sha256, sha256v};
pub use similarity::{
    DistanceThresholds, hamming_distance, is_similar, jaccard_distance, manhattan_distance,
};
pub use sized_iterator::SizedIterator;
pub use small_boolean::SmallBoolean;
pub use swap_or_not::compute_shuffled_index;
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
