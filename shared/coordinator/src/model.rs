use crate::model_extra_data::{CHECKPOINT_DATA_MAX_LEN, CheckpointData};
use crate::{SOLANA_MAX_STRING_LEN, coordinator::SOLANA_MAX_URL_STRING_LEN};

use anchor_lang::{
    AnchorDeserialize, AnchorSerialize, InitSpace,
    prelude::{borsh, msg},
};
use bytemuck::{Zeroable, ZeroableInOption};
use psyche_core::{FixedString, FixedVec, Shuffle, TokenSize};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Opaque byte blob holding borsh-serialized [`CheckpointData`].
pub type CheckpointBytes = FixedVec<u8, CHECKPOINT_DATA_MAX_LEN>;

#[derive(
    Clone, Debug, Copy, Zeroable, AnchorDeserialize, AnchorSerialize, Serialize, Deserialize, TS,
)]
#[repr(C)]
pub enum Model {
    LLM(LLM),
}

unsafe impl ZeroableInOption for Model {}

#[derive(
    Clone,
    Debug,
    Copy,
    Zeroable,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
    TS,
    PartialEq,
)]
#[repr(C)]
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

#[derive(
    Clone,
    Debug,
    Copy,
    Zeroable,
    AnchorDeserialize,
    AnchorSerialize,
    Serialize,
    Deserialize,
    InitSpace,
    PartialEq,
    TS,
)]
#[repr(C)]
pub enum LLMTrainingDataType {
    Pretraining,
    Finetuning,
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    TS,
)]
#[repr(C)]
#[allow(clippy::large_enum_variant)]
#[derive(Default)]
pub enum LLMTrainingDataLocation {
    #[default]
    Dummy,
    Server(FixedString<{ SOLANA_MAX_STRING_LEN }>),
    Local(FixedString<{ SOLANA_MAX_URL_STRING_LEN }>),
    Http(HttpLLMTrainingDataLocation),
    /// link to a JSON file that deserializes to a Vec<LLMTrainingDataLocationAndWeight>
    WeightedHttp(FixedString<{ SOLANA_MAX_URL_STRING_LEN }>),
    Preprocessed(FixedString<{ SOLANA_MAX_URL_STRING_LEN }>),
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    TS,
)]
#[repr(C)]
#[allow(clippy::large_enum_variant)]
pub struct HttpLLMTrainingDataLocation {
    pub location: HttpTrainingDataLocation,
    pub token_size_in_bytes: TokenSize,
    pub shuffle: Shuffle,
}

/// these are deserialized from JSON
#[derive(Serialize, Deserialize, Clone, Debug, Copy)]
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

impl<const N: usize> From<LLMTrainingDataLocation>
    for FixedVec<LLMTrainingDataLocationAndWeight, N>
{
    fn from(location: LLMTrainingDataLocation) -> Self {
        FixedVec::from_iter([LLMTrainingDataLocationAndWeight {
            location,
            weight: 1.0,
        }])
    }
}

impl LLMTrainingDataLocationAndWeight {
    pub fn new(location: LLMTrainingDataLocation, weight: f32) -> Self {
        Self { location, weight }
    }
}

/// NOTE: Support for Vecs of URLs is not enabled because of the large size it would support.
#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    InitSpace,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    TS,
)]
#[repr(C)]
#[allow(clippy::large_enum_variant)]
pub enum HttpTrainingDataLocation {
    SingleUrl(FixedString<{ SOLANA_MAX_URL_STRING_LEN }>),
    NumberedFiles {
        url_template: FixedString<{ SOLANA_MAX_STRING_LEN }>,
        start_index: u32,
        n_left_pad_zeros: u8,
        num_files: u32,
    },
    Gcp {
        bucket_name: FixedString<{ SOLANA_MAX_STRING_LEN }>,

        /// 0 len === no filter
        filter_directory: FixedString<{ SOLANA_MAX_URL_STRING_LEN }>,
    },
}

#[derive(
    AnchorSerialize, AnchorDeserialize, Serialize, Deserialize, Clone, Debug, Zeroable, Copy, TS,
)]
#[repr(C)]
pub struct LLM {
    pub max_seq_len: u32,
    pub cold_start_warmup_steps: u32,
    pub checkpoint_source: CheckpointSource,
    #[serde(default)]
    pub checkpoint_data: FixedVec<u8, { CHECKPOINT_DATA_MAX_LEN }>,
}

impl LLM {
    pub fn dummy() -> Self {
        Self {
            checkpoint_source: CheckpointSource::Stored,
            checkpoint_data: CheckpointData::Dummy.to_fixed_vec(),
            max_seq_len: 2048,
            cold_start_warmup_steps: 0,
        }
    }

    /// Decode the opaque checkpoint bytes into a [`CheckpointData`].
    pub fn decode_checkpoint(&self) -> Option<CheckpointData> {
        CheckpointData::from_fixed_vec(&self.checkpoint_data).ok()
    }
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    Serialize,
    Deserialize,
    Clone,
    Debug,
    Zeroable,
    Copy,
    TS,
    PartialEq,
    Default,
)]
#[repr(C)]
pub enum CheckpointSource {
    Ephemeral,
    #[default]
    Stored,
    P2P,
}

impl std::fmt::Display for CheckpointSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckpointSource::Ephemeral => write!(f, "Ephemeral"),
            CheckpointSource::Stored => write!(f, "Stored"),
            CheckpointSource::P2P => write!(f, "P2P"),
        }
    }
}

impl Model {
    pub fn check(&self) -> bool {
        match self {
            Model::LLM(llm) => {
                if llm.max_seq_len == 0 {
                    msg!("model check failed: max_seq_len is 0.");
                    return false;
                }

                if matches!(llm.checkpoint_source, CheckpointSource::Ephemeral) {
                    msg!("model check failed: bad checkpoint (ephemeral)");
                    return false;
                }

                let bad_checkpoint = match CheckpointData::from_fixed_vec(&llm.checkpoint_data) {
                    Ok(CheckpointData::Dummy) => false,
                    Ok(CheckpointData::Hub { repo_id, .. }) => repo_id.is_empty(),
                    Ok(CheckpointData::Gcs { bucket, .. }) => bucket.is_empty(),
                    Err(_) => {
                        msg!("model check failed: could not deserialize checkpoint data");
                        true
                    }
                };

                if bad_checkpoint {
                    msg!("model check failed: bad checkpoint");
                    return false;
                }

                true
            }
        }
    }
}
