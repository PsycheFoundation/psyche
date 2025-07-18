use crate::traits::{LengthKnownDataProvider, TokenizedDataProvider};
use anyhow::{Result, anyhow};
use psyche_core::{BatchId, ClosedInterval, Shuffle};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

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
    let num_providers = weights.len();
    let mut dataset_index = Vec::with_capacity(n_samples);
    let mut dataset_sample_index = Vec::with_capacity(n_samples);

    let mut total_samples_drawn = vec![0u64; num_providers];
    let mut next_unique_index = vec![0u64; num_providers];
    let mut is_exhausted = vec![false; num_providers];

    for sample_idx in 0..n_samples {
        let sample_idx_float = (sample_idx as f64).max(1.0);

        // select provider based on weighted error
        let mut max_error = f64::NEG_INFINITY;
        let mut chosen_provider_idx = 0;
        for i in 0..num_providers {
            if dataset_sizes[i] == 0 {
                continue;
            }
            let error = weights[i] * sample_idx_float - total_samples_drawn[i] as f64;
            if error > max_error {
                max_error = error;
                chosen_provider_idx = i;
            }
        }

        // determine the sample index
        let provider_size = dataset_sizes[chosen_provider_idx] as u64;
        let sample_to_yield: u64;

        if !is_exhausted[chosen_provider_idx] {
            sample_to_yield = next_unique_index[chosen_provider_idx];
            next_unique_index[chosen_provider_idx] += 1;

            if next_unique_index[chosen_provider_idx] == provider_size {
                is_exhausted[chosen_provider_idx] = true;
            }
        } else {
            sample_to_yield = total_samples_drawn[chosen_provider_idx] % provider_size;
        }

        dataset_index.push(chosen_provider_idx);
        dataset_sample_index.push(sample_to_yield);

        total_samples_drawn[chosen_provider_idx] += 1;
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
