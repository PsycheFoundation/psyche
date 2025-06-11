use std::{
    cell::UnsafeCell,
    cmp::min,
    mem,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::traits::{LengthKnownDataProvider, TokenizedDataProvider};
use anyhow::{anyhow, Result};
use psyche_core::{BatchId, ClosedInterval, Shuffle};
use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::{ChaCha20Rng, ChaCha8Rng};
use rayon::{
    iter::{
        repeat, IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator,
        ParallelIterator,
    },
    slice::ParallelSliceMut,
};
use rip_shuffle::RipShuffleParallel;

pub mod http;
pub struct WeightedDataProvider<T: TokenizedDataProvider + LengthKnownDataProvider> {
    providers: Vec<T>,
    dataset_index: Vec<usize>,
    dataset_sample_index: Vec<u64>,
}

pub enum Providers<T: TokenizedDataProvider + LengthKnownDataProvider> {
    /// Weights will be normalized to their sum. e.g. weights 1.0, 1.0, 2.0 will normalize to 0.25, 0.25, 0.5
    ExplicitlyWeighted(Vec<(T, f64)>),
    /// Weights will be derived from dataset lengths, and normalized.
    LengthWeighted(Vec<T>),
}

impl<T: TokenizedDataProvider + LengthKnownDataProvider> From<Vec<(T, f64)>> for Providers<T> {
    fn from(value: Vec<(T, f64)>) -> Self {
        Self::ExplicitlyWeighted(value)
    }
}

impl<T: TokenizedDataProvider + LengthKnownDataProvider> From<Vec<T>> for Providers<T> {
    fn from(value: Vec<T>) -> Self {
        Self::LengthWeighted(value)
    }
}

impl<T: TokenizedDataProvider + LengthKnownDataProvider> Providers<T> {
    pub fn weights(&self) -> Vec<f64> {
        match self {
            Self::ExplicitlyWeighted(w) => {
                normalize(&w.iter().map(|(_, w)| *w).collect::<Vec<_>>())
            }
            Self::LengthWeighted(w) => {
                let dataset_lengths: Vec<f64> =
                    w.iter().map(|p| p.num_sequences() as f64).collect();
                normalize(&dataset_lengths)
            }
        }
    }
    pub fn providers(self) -> Vec<T> {
        match self {
            Self::ExplicitlyWeighted(w) => w.into_iter().map(|(p, _)| p).collect(),
            Self::LengthWeighted(w) => w,
        }
    }
}

impl<T: TokenizedDataProvider + LengthKnownDataProvider> WeightedDataProvider<T> {
    pub fn new(weighted_providers: impl Into<Providers<T>>, shuffle_kind: Shuffle) -> Self {
        let weighted_providers = weighted_providers.into();
        // normalize weights if provided, otherwise use dataset lengths as weights
        let weights = weighted_providers.weights();
        let providers = weighted_providers.providers();
        assert_eq!(
            providers.len(),
            weights.len(),
            "Number of providers must match number of weights"
        );

        let num_samples = providers.iter().map(|p| p.num_sequences()).sum();

        let dataset_lengths: Vec<usize> = providers.iter().map(|p| p.num_sequences()).collect();
        let samples_per_epoch: usize = dataset_lengths.iter().sum();
        let num_epochs = (num_samples as f64 / samples_per_epoch as f64).ceil() as usize;

        let (mut dataset_index, mut dataset_sample_index) =
            build_weighted_index(samples_per_epoch, &weights, &dataset_lengths);

        if let Shuffle::Seeded(random_seed) = shuffle_kind {
            let mut rng = ChaCha8Rng::from_seed(random_seed);
            shuffle(&mut dataset_index, &mut dataset_sample_index, &mut rng);
        }

        let mut full_dataset_index = Vec::with_capacity(num_samples);
        let mut full_dataset_sample_index = Vec::with_capacity(num_samples);

        for _ in 0..num_epochs {
            full_dataset_index.extend_from_slice(&dataset_index);
            full_dataset_sample_index.extend_from_slice(&dataset_sample_index);
        }

        // set back to requested number of samples
        full_dataset_index.truncate(num_samples);
        full_dataset_sample_index.truncate(num_samples);

        tracing::info!(num_samples = num_samples, "Created weighted data provider",);

        Self {
            providers,
            dataset_index: full_dataset_index,
            dataset_sample_index: full_dataset_sample_index,
        }
    }

    fn get_sample_info(&self, index: u64) -> (usize, u64) {
        let idx = index as usize;
        if idx >= self.dataset_index.len() {
            return (0, 0);
        }
        let dataset_idx = self.dataset_index[idx];
        let sample_idx = self.dataset_sample_index[idx];
        (dataset_idx, sample_idx)
    }
}

impl<T: TokenizedDataProvider + LengthKnownDataProvider> LengthKnownDataProvider
    for WeightedDataProvider<T>
{
    fn num_sequences(&self) -> usize {
        self.dataset_index.len()
    }
}

impl<T: TokenizedDataProvider + LengthKnownDataProvider + Send> TokenizedDataProvider
    for WeightedDataProvider<T>
{
    async fn get_samples(&mut self, data_ids: BatchId) -> Result<Vec<Vec<i32>>> {
        let mut provider_requests: Vec<Vec<(usize, u64)>> = vec![Vec::new(); self.providers.len()];

        for (original_idx, id) in data_ids.iter().enumerate() {
            let (provider_idx, sample_idx) = self.get_sample_info(id);
            provider_requests[provider_idx].push((original_idx, sample_idx));
        }

        // all results in their original order
        let mut results = vec![Vec::new(); data_ids.len()];

        for (provider_idx, requests) in provider_requests.iter().enumerate() {
            if !requests.is_empty() {
                let mut sorted_requests = requests.clone();
                sorted_requests.sort_by_key(|&(_, idx)| idx); // find contiguous ranges

                let mut ranges: Vec<Vec<(usize, u64)>> = Vec::new();
                let mut current_range = vec![sorted_requests[0]];

                for &(orig_idx, idx) in &sorted_requests[1..] {
                    let (_, prev_idx) = current_range.last().unwrap();
                    if idx == prev_idx + 1 {
                        current_range.push((orig_idx, idx));
                    } else {
                        ranges.push(current_range);
                        current_range = vec![(orig_idx, idx)];
                    }
                }
                ranges.push(current_range);

                for range in ranges {
                    let start = range.first().unwrap().1;
                    let end = range.last().unwrap().1;
                    let batch_id = BatchId(ClosedInterval { start, end });

                    let range_samples = self.providers[provider_idx].get_samples(batch_id).await?;
                    for ((orig_idx, _), sample) in range.iter().zip(range_samples) {
                        results[*orig_idx] = sample;
                    }
                }
            }
        }

        if results.iter().any(|v| v.is_empty()) {
            return Err(anyhow!("Failed to get all requested samples"));
        }

        Ok(results)
    }
}

fn normalize(weights: &[f64]) -> Vec<f64> {
    let sum: f64 = weights.iter().sum();
    weights.iter().map(|w| w / sum).collect()
}

// todo: handle various edge cases (e.g. dataset_sizes memebers == 0 n_samples == 0 etc)
fn build_weighted_index(
    n_samples: usize,
    weights: &[f64],
    dataset_sizes: &[usize],
) -> (Vec<usize>, Vec<u64>) {
    // todo: improve this computation to ensure we don't need to compute this sum
    // and maybe try to gaurantee norm_weights add to 1
    let weights_sum: f64 = weights.par_iter().sum();
    let norm_weights: Vec<f64> = weights
        .par_iter()
        .map(|weight| weight / weights_sum)
        .collect();

    let data_idx_sequences = dataset_sizes
        .par_iter()
        .zip(norm_weights.par_iter())
        .map(|(dataset_size, norm_weight)| {
            let mut data_seq: Vec<_> = (0..*dataset_size).into_par_iter().collect();
            //todo: this is so bad T_T do we need to cryptanalyze this?
            let mut rng = ChaCha20Rng::seed_from_u64(unsafe { mem::transmute(*norm_weight) });
            data_seq.par_shuffle(&mut rng);

            data_seq
        })
        .collect::<Vec<_>>();


    // Super-involved part of the algorithm. The idea here is that we want to we first want to create a "mask"
    // of dataset indexes that samples from the datasets based on their weights. for example, it dataset 0 has weight 0.75 and
    // dataset 1 has weight 0.25, and we want 4 samples, then the mask might look like [0, 1, 0, 0].
    // continuing from the above example, every time we sample from dataset 0, we want to take the "next" sample from that dataset
    // (the order of the samples has been randomized above) until we get to the end, at which point we loop around.
    // It's important we go through the entire dataset before looping aroud for small datasets.
    // Thus, if dataset 0 has samples [1, 0] and dataset 1 [0], then we want around final result to look like
    // [0_1, 1_0, 0_0, 0_1] where the first number is the dataset index and the second is the sample in the dataset.
    // therefore, first we generate the mask, and next we want to sample from each dataset sequentially. This is relatively
    // easy to accomplish serially - just have an iter().cycle() into each of the elements of data_idx_sequences. Not so
    // if we want to parallelize the algorithm though. Therefore, when we generate the mask, we generate it as a Vec<(usize, usize)>, where the
    // first element of the tuple is an ascending index into the dataset sequence, and the second is the index of the dataset sequence.
    // next, we want to distribute these tuples randomly thoughout randomized-ordered mask, but such that the index of each
    // tuple for the same dataset is monotonically increasing. thus: [(0, 0), (0, 1) (1, 0), (2, 0)], where the second element of each tuple
    // is the dataset index, and the first is the index into it, which is monotonically increasing for any given dataset.
    // to accomplish this, we first generate the mask in-order: [(0, 0), (1, 0), (2, 0), (0, 1)]. We then generate a list of indices
    // which we then scamble the order of, eg [3, 0, 2, 1]. We then divide the indices into regions corresponding to each dataset
    //, and then sort each sub-region, thus: [0, 2, 3, 1]. We then take our ordered mask, and write to the final resulting scrambled mask
    // each tuple to the index indicated by the index in our indices Vec. Doing this with the above example, we get our final result
    // [(0, 0), (0, 1) (1, 0), (2, 0)]. We then use this scrambled mask to sample from data_idx_sequences like such: data_idx_sequences[tuple.1][tuple.0]
    let mut rng = ChaCha20Rng::seed_from_u64(unsafe { mem::transmute(weights_sum) });

    let mut mask = norm_weights
        .par_iter()
        .enumerate()
        .flat_map(|(idx, weight)| {
            repeat(idx)
                .take((weight * n_samples as f64) as usize)
                .enumerate()
                .map(|(idx, val)| (idx, val))
        })
        .collect::<Vec<_>>();

    mask.truncate(n_samples);

    if mask.len() < n_samples {
        let val = mask[mask.len() - 1].1;
        let last_idx = mask[mask.len() - 1].0;
        let it = std::iter::repeat(val)
            .take(n_samples - mask.len())
            .enumerate()
            .map(|(idx, val)| (last_idx + 1 + idx, val));
        mask.extend(it);
    }

    let mut indexes = (0..n_samples).into_par_iter().collect::<Vec<_>>();
    indexes.par_shuffle(&mut rng);

    let mut accum = 0;

    for (idx, w) in norm_weights.iter().enumerate() {
        if accum >= n_samples {
            break;
        }

        if idx != norm_weights.len() - 1 {
            let size = (w * n_samples as f64) as usize;

            indexes[accum..min(accum + size, n_samples)].par_sort();
            accum += size;
        } else {
            indexes[accum..].par_sort();
        }
    }

    let mask = unsafe {
        // use atomics to make rust happy - these relaxed stores should just compile down to regular stores (i hope)
        let mut new_mask = Vec::<(AtomicUsize, AtomicUsize)>::with_capacity(n_samples);
        new_mask.set_len(n_samples);

        mask.into_par_iter()
            .zip(indexes.into_par_iter())
            .for_each(|(val, idx)| {
                new_mask[idx].0.store(val.0, Ordering::Relaxed);
                new_mask[idx].1.store(val.1, Ordering::Relaxed);
            });

        new_mask
    };

    let dataset_idx_and_sample_idx = mask
        .par_iter()
        .map(|(idx, i)| {
            let seq_len = data_idx_sequences[i.load(Ordering::Relaxed)].len();
            (
                i.load(Ordering::Relaxed),
                data_idx_sequences[i.load(Ordering::Relaxed)][idx.load(Ordering::Relaxed) % seq_len]
                    as u64,
            )
        })
        .collect::<(Vec<_>, Vec<_>)>();

    dataset_idx_and_sample_idx
}

fn shuffle<T: Rng>(dataset_index: &mut [usize], dataset_sample_index: &mut [u64], rng: &mut T) {
    let n = dataset_index.len();
    for i in (1..n).rev() {
        let j = rng.gen_range(0..=i);
        dataset_index.swap(i, j);
        dataset_sample_index.swap(i, j);
    }
}
