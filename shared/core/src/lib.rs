#![allow(unexpected_cfgs)]

mod batch_id;
mod bounded_queue;
mod cancellable_barrier;
mod data_shuffle;
mod deterministic_shuffle;
mod interval_tree;
mod lcg;
mod lr_scheduler;
mod running_average;
mod similarity;
mod sized_iterator;
mod token_size;

pub use batch_id::BatchId;
pub use bounded_queue::BoundedQueue;
pub use cancellable_barrier::{CancellableBarrier, CancelledBarrier};
pub use data_shuffle::Shuffle;
pub use deterministic_shuffle::deterministic_shuffle;
pub use interval_tree::{ClosedInterval, IntervalTree};
pub use lcg::LCG;
pub use lr_scheduler::*;
pub use running_average::RunningAverage;
pub use similarity::{
    hamming_distance, is_similar, jaccard_distance, manhattan_distance, DistanceThresholds, DistanceError
};
pub use sized_iterator::SizedIterator;
pub use token_size::TokenSize;

#[cfg(test)]
mod tests {
    /// A lot of the code here assumes that usize is u64. This should be true on every platform we support.
    #[test]
    fn test_check_type_assumptions() {
        assert_eq!(size_of::<u64>(), size_of::<usize>());
    }
}
