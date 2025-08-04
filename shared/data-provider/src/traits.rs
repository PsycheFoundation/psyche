use anyhow::Result;
use psyche_core::BatchId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct TokenizedData {
    pub input_ids: Vec<i32>,
    pub labels: Option<Vec<i32>>,
    pub position_ids: Option<Vec<i32>>,
    pub sequence_lengths: Option<Vec<i32>>,
}

impl TokenizedData {
    pub fn from_input_ids(input_ids: Vec<i32>) -> Self {
        Self {
            input_ids,
            labels: None,
            position_ids: None,
            sequence_lengths: None,
        }
    }

    pub fn new(
        input_ids: Vec<i32>,
        labels: Option<Vec<i32>>,
        position_ids: Option<Vec<i32>>,
        sequence_lengths: Option<Vec<i32>>,
    ) -> Self {
        Self {
            input_ids,
            labels,
            position_ids,
            sequence_lengths,
        }
    }

    pub fn empty() -> Self {
        Self {
            input_ids: vec![],
            labels: None,
            position_ids: None,
            sequence_lengths: None,
        }
    }
}

pub trait TokenizedDataProvider {
    fn get_samples(
        &mut self,
        data_ids: BatchId,
    ) -> impl std::future::Future<Output = Result<Vec<TokenizedData>>> + Send;
}

pub trait LengthKnownDataProvider {
    fn num_sequences(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.num_sequences() == 0
    }
}
