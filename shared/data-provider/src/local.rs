use anyhow::{anyhow, bail, Result};
use psyche_core::{BatchId, Shuffle, TokenSize};
use rand::seq::SliceRandom;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::fs;
use tracing::info;

use crate::{
    file_extensions::DATA_FILE_EXTENSIONS,
    traits::{LengthKnownDataProvider, TokenizedDataProvider},
};

fn mmap_file(p: &std::path::PathBuf) -> Result<memmap2::Mmap> {
    let file = std::fs::File::open(p)?;
    let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
    Ok(mmap)
}

struct SequencePointer {
    file_index: usize,
    byte_offset: usize,
}

pub struct LocalDataProvider {
    data_files: Vec<memmap2::Mmap>,
    sequences: Vec<SequencePointer>,
    seq_len: usize,
    token_size_in_bytes: TokenSize,
}

impl LengthKnownDataProvider for LocalDataProvider {
    fn len(&self) -> usize {
        self.sequences.len()
    }
}
impl LocalDataProvider {
    pub fn new_from_directory(
        dir: impl AsRef<std::path::Path>,
        token_size_in_bytes: TokenSize,
        num_tokens_per_sequence: usize, // num tokens per sequence
        shuffle: Shuffle,
    ) -> Result<Self> {
        let dir = std::fs::canonicalize(&dir)
            .map_err(|e| anyhow!("Failed to open data directory {:?}: {e}", dir.as_ref()))?;
        let mut bin_files = vec![];
        for file in std::fs::read_dir(&dir)
            .map_err(|e| anyhow!("couldn't load training data from {}: {e}", dir.display()))?
            .flatten()
        {
            let file = file.path();
            if let Some(extension) = file.extension().and_then(|s| s.to_str()) {
                if DATA_FILE_EXTENSIONS.contains(&extension) {
                    bin_files.push(file)
                }
            }
        }
        let data_files = bin_files
            .iter()
            .map(mmap_file)
            .collect::<Result<Vec<_>>>()?;

        if data_files.is_empty() {
            bail!("No training data files in directory {:?}", dir);
        }

        info!(
            "Loaded {} files ({}) of training data from directory {}",
            bin_files.len(),
            bin_files
                .iter()
                .map(|f| fs::metadata(f).unwrap().len())
                .sum::<u64>(),
            dir.display()
        );

        let deterministic_rng = match shuffle {
            Shuffle::Seeded(random_seed) => Some(ChaCha8Rng::from_seed(random_seed)),
            Shuffle::DontShuffle => None,
        };
        let seq_len_in_bytes = num_tokens_per_sequence * usize::from(token_size_in_bytes);

        let sequences: Vec<SequencePointer> = {
            let mut all_indexes: Vec<_> = data_files
                .iter()
                .enumerate()
                // find every sequence in every file
                .flat_map(|(file_index, current_tokens)| {
                    (0..current_tokens.len()
                        - (seq_len_in_bytes + usize::from(token_size_in_bytes))) // +1 token for pretraining data!
                        .step_by(seq_len_in_bytes)
                        .map(move |byte_offset| SequencePointer {
                            file_index,
                            byte_offset,
                        })
                })
                .collect();
            // and shuffle the whole collection, to avoid bias from a specific file
            if let Some(mut deterministic_rng) = deterministic_rng {
                all_indexes.shuffle(&mut deterministic_rng);
            }
            all_indexes
        };

        Ok(Self {
            data_files,
            sequences,
            seq_len: num_tokens_per_sequence,
            token_size_in_bytes,
        })
    }

    fn internal_get_samples(&self, data_ids: &[BatchId]) -> Result<Vec<Vec<i32>>> {
        let mut ret: Vec<_> = Vec::new();
        for data_id in data_ids {
            let SequencePointer {
                byte_offset,
                file_index,
            } = self
                .sequences
                .get(u64::from(*data_id) as usize)
                .ok_or_else(|| {
                    anyhow!(
                        "index {data_id} is out of bounds, we only have {} samples.",
                        self.sequences.len()
                    )
                })?;

            let file = &self.data_files[*file_index];
            let data_len = usize::from(self.token_size_in_bytes) * (self.seq_len + 1);
            let data = &file[*byte_offset..*byte_offset + data_len];

            let tokens: Vec<i32> = data
                .chunks(self.token_size_in_bytes.into())
                .map(|t| {
                    use TokenSize::*;
                    match self.token_size_in_bytes {
                        TwoBytes => u16::from_le_bytes(t.try_into().unwrap()) as i32,
                        FourBytes => u32::from_le_bytes(t.try_into().unwrap()) as i32,
                    }
                })
                .collect();
            ret.push(tokens);
        }
        Ok(ret)
    }
}

impl TokenizedDataProvider for LocalDataProvider {
    async fn get_samples(&mut self, data_ids: &[BatchId]) -> Result<Vec<Vec<i32>>> {
        self.internal_get_samples(data_ids)
    }
}

pub struct LocalDataProviderIter {
    provider: LocalDataProvider,
    current_index: usize,
}

impl Iterator for LocalDataProviderIter {
    type Item = Vec<i32>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index < self.provider.len() {
            let result = self
                .provider
                .internal_get_samples(&[BatchId::from_u64(self.current_index as u64)])
                .unwrap()
                .pop()
                .unwrap();
            self.current_index += 1;
            Some(result)
        } else {
            None
        }
    }
}

impl IntoIterator for LocalDataProvider {
    type Item = Vec<i32>;
    type IntoIter = LocalDataProviderIter;

    fn into_iter(self) -> Self::IntoIter {
        LocalDataProviderIter {
            provider: self,
            current_index: 0,
        }
    }
}
