use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Shuffle, TokenSize};

#[derive(Clone, Serialize, Deserialize, TS, Default, Debug)]
pub struct WitnessMetadata {
    pub step: u32,
    pub tokens_per_sec: f32,
    pub bandwidth_per_sec: f32,
    pub loss: f32,
    pub evals: Vec<WitnessEvalResult>,
    pub prompt_results: Vec<i32>,
    pub prompt_index: u8,
    pub efficency: f32,
}

#[derive(Clone, Serialize, Deserialize, TS, Default, Debug)]
pub struct WitnessEvalResult {
    pub name: String,
    pub value: f32,
}

#[derive(Clone, Debug, Copy, Serialize, Deserialize, TS, PartialEq)]
pub enum LLMArchitecture {
    HfLlama,
    HfDeepseek,
    HfAuto,
    Torchtitan,
}

impl std::fmt::Display for LLMArchitecture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMArchitecture::HfLlama => f.write_str("HfLlama"),
            LLMArchitecture::HfDeepseek => f.write_str("HfDeepseek"),
            LLMArchitecture::HfAuto => f.write_str("HfAuto"),
            LLMArchitecture::Torchtitan => f.write_str("Torchtitan"),
        }
    }
}

#[derive(Clone, Debug, Copy, Serialize, Deserialize, PartialEq, TS)]
pub enum LLMTrainingDataType {
    Pretraining,
    Finetuning,
}

#[derive(Serialize, Deserialize, Clone, Debug, TS, Default)]
pub enum LLMTrainingDataLocation {
    #[default]
    Dummy,
    Server(String),
    Local(String),
    Http(HttpLLMTrainingDataLocation),
    /// link to a JSON file that deserializes to a Vec<LLMTrainingDataLocationAndWeight>
    WeightedHttp(String),
    Preprocessed(String),
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
pub struct HttpLLMTrainingDataLocation {
    pub location: HttpTrainingDataLocation,
    pub token_size_in_bytes: TokenSize,
    pub shuffle: Shuffle,
}

/// these are deserialized from JSON
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LLMTrainingDataLocationAndWeight {
    pub location: LLMTrainingDataLocation,
    pub weight: f32,
}

impl Default for LLMTrainingDataLocationAndWeight {
    fn default() -> Self {
        Self {
            location: Default::default(),
            weight: 1.0,
        }
    }
}

impl From<LLMTrainingDataLocation> for Vec<LLMTrainingDataLocationAndWeight> {
    fn from(location: LLMTrainingDataLocation) -> Self {
        vec![LLMTrainingDataLocationAndWeight {
            location,
            weight: 1.0,
        }]
    }
}

impl LLMTrainingDataLocationAndWeight {
    pub fn new(location: LLMTrainingDataLocation, weight: f32) -> Self {
        Self { location, weight }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, TS)]
pub enum HttpTrainingDataLocation {
    SingleUrl(String),
    NumberedFiles {
        url_template: String,
        start_index: u32,
        n_left_pad_zeros: u8,
        num_files: u32,
    },
    Gcp {
        bucket_name: String,

        /// 0 len === no filter
        filter_directory: Option<String>,
    },
}
