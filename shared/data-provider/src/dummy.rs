use crate::{LengthKnownDataProvider, TokenizedData, traits::TokenizedDataProvider};
use anyhow::{Result, bail};
use psyche_core::{BatchId, TokenSize};

pub struct DummyDataProvider {
    seq_len: usize,
    token_size_in_bytes: TokenSize,
    num_sequences: u64,
}

impl DummyDataProvider {
    pub fn new(
        token_size_in_bytes: TokenSize,
        num_tokens_per_sequence: usize, // num tokens per sequence
        num_sequences: u64,
    ) -> Self {
        Self {
            seq_len: num_tokens_per_sequence,
            token_size_in_bytes,
            num_sequences,
        }
    }

    fn internal_get_samples(&self, num_samples: usize) -> Result<Vec<TokenizedData>> {
        let mut ret: Vec<_> = Vec::new();
        for _ in 0..num_samples {
            let data_len = usize::from(self.token_size_in_bytes) * self.seq_len;
            let data = vec![0; data_len];

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
            ret.push(TokenizedData::from_input_ids(tokens));
        }
        Ok(ret)
    }
}

impl TokenizedDataProvider for DummyDataProvider {
    async fn get_samples(&mut self, data_ids: BatchId) -> Result<Vec<TokenizedData>> {
        for id in data_ids.iter() {
            if id > self.num_sequences {
                bail!("id {id} > self.num_sequences {}", self.num_sequences)
            }
        }
        self.internal_get_samples(data_ids.len())
    }
}

impl LengthKnownDataProvider for DummyDataProvider {
    fn num_sequences(&self) -> usize {
        self.num_sequences as usize
    }
}
