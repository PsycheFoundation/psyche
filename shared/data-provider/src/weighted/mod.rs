use std::{mem, sync::atomic::AtomicUsize};

use crate::traits::{LengthKnownDataProvider, TokenizedDataProvider};
use anyhow::{anyhow, Result};
use psyche_core::{BatchId, ClosedInterval, Shuffle};
use rand::{seq::SliceRandom, Rng, SeedableRng};
use rand_chacha::{ChaCha20Rng, ChaCha8Rng};
use rayon::iter::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};

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

fn build_weighted_index(
    n_samples: usize,
    weights: &[f64],
    dataset_sizes: &[usize],
) -> (Vec<usize>, Vec<u64>) {
    // todo: improve this computation to ensure we don't need to compute this sum
    // and maybe try to gaurantee norm_weights add to 1
    let weights_sum: f64 = weights.iter().sum();
    let norm_weights: Vec<f64> = weights.iter().map(|weight| weight / weights_sum).collect();

    let data_idx_sequences = dataset_sizes
        .iter()
        .zip(norm_weights.iter())
        .map(|(dataset_size, norm_weight)| {
            let mut data_seq: Vec<_> = (0..*dataset_size).collect();
            //todo: this is so bad T_T do we need to cryptanalyze this?
            let mut rng = ChaCha20Rng::seed_from_u64(unsafe { mem::transmute(*norm_weight) });
            data_seq.shuffle(&mut rng);

            data_seq
        })
        .collect::<Vec<_>>();

    let mut dataset_index = Vec::with_capacity(n_samples);
    let mut dataset_sample_index = Vec::with_capacity(n_samples);

    let mut mask = norm_weights
        .iter()
        .enumerate()
        .flat_map(|(idx, weight)| std::iter::repeat(idx).take((weight * n_samples as f64) as usize))
        .collect::<Vec<_>>();

    mask.truncate(n_samples);

    if mask.len() < n_samples {
        let it = std::iter::repeat(mask[mask.len() - 1]).take(n_samples - mask.len());
        mask.extend(it);
    }

    let mut rng = ChaCha20Rng::seed_from_u64(unsafe { mem::transmute(weights_sum) });
    mask.shuffle(&mut rng);

    let mut iters = data_idx_sequences.iter().map(|subvec| subvec.iter().cycle()).collect::<Vec<_>>();

    for i in mask {
        dataset_index.push(i);
        dataset_sample_index.push(*iters[i].next().unwrap() as u64);
    }

    (dataset_index, dataset_sample_index)
}

fn shuffle<T: Rng>(dataset_index: &mut [usize], dataset_sample_index: &mut [u64], rng: &mut T) {
    let n = dataset_index.len();
    for i in (1..n).rev() {
        let j = rng.gen_range(0..=i);
        dataset_index.swap(i, j);
        dataset_sample_index.swap(i, j);
    }
}
