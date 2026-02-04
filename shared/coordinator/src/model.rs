use crate::{SOLANA_MAX_STRING_LEN, coordinator::SOLANA_MAX_URL_STRING_LEN};

use anchor_lang::{
    AnchorDeserialize, AnchorSerialize, InitSpace,
    prelude::{borsh, msg},
};
use bytemuck::{Zeroable, ZeroableInOption};
use psyche_core::{
    ConstantLR, FixedString, FixedVec, LearningRateSchedule, OptimizerDefinition, Shuffle,
    TokenSize,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    pub architecture: LLMArchitecture,
    pub checkpoint: Checkpoint,
    pub data_type: LLMTrainingDataType,
    pub data_location: LLMTrainingDataLocation,
    pub lr_schedule: LearningRateSchedule,
    pub optimizer: OptimizerDefinition,
}

impl LLM {
    pub fn dummy() -> Self {
        Self {
            architecture: LLMArchitecture::HfLlama,
            checkpoint: Checkpoint::Dummy(HubRepo::dummy()),
            data_location: LLMTrainingDataLocation::default(),
            data_type: LLMTrainingDataType::Pretraining,
            lr_schedule: LearningRateSchedule::Constant(ConstantLR::default()),
            max_seq_len: 2048,
            optimizer: OptimizerDefinition::Dummy,
            cold_start_warmup_steps: 0,
        }
    }
}

#[derive(
    Clone,
    Debug,
    Copy,
    AnchorDeserialize,
    AnchorSerialize,
    InitSpace,
    Serialize,
    Deserialize,
    PartialEq,
    TS,
)]
pub struct HubRepo {
    pub repo_id: FixedString<{ SOLANA_MAX_STRING_LEN }>,
    pub revision: Option<FixedString<{ SOLANA_MAX_STRING_LEN }>>,
}

impl HubRepo {
    pub fn dummy() -> Self {
        Self {
            repo_id: FixedString::new(),
            revision: None,
        }
    }
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
pub enum Checkpoint {
    Ephemeral,
    Dummy(HubRepo),
    Hub(HubRepo),
    P2P(HubRepo),
}

impl std::fmt::Display for Checkpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Checkpoint::Dummy(_hub_repo) => write!(f, "Dummy"),
            Checkpoint::Ephemeral => write!(f, "Ephemeral"),
            Checkpoint::Hub(hub_repo) => write!(f, "{}", &hub_repo.repo_id),
            Checkpoint::P2P(hub_repo) => {
                write!(f, "P2P - Hub repo: {}", &hub_repo.repo_id)
            }
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

                let bad_data_location = match llm.data_location {
                    LLMTrainingDataLocation::Dummy => false,
                    LLMTrainingDataLocation::Server(url) => url.is_empty(),
                    LLMTrainingDataLocation::Local(_) => false,
                    LLMTrainingDataLocation::Http(HttpLLMTrainingDataLocation {
                        location, ..
                    }) => match location {
                        HttpTrainingDataLocation::SingleUrl(url) => url.is_empty(),
                        HttpTrainingDataLocation::NumberedFiles {
                            url_template,
                            num_files,
                            ..
                        } => url_template.is_empty() || num_files == 0,
                        HttpTrainingDataLocation::Gcp { bucket_name, .. } => bucket_name.is_empty(),
                    },
                    LLMTrainingDataLocation::WeightedHttp(url) => url.is_empty(),
                    LLMTrainingDataLocation::Preprocessed(url) => url.is_empty(),
                };
                if bad_data_location {
                    msg!("model check failed: bad LLM training data location.");
                    return false;
                }
                let bad_checkpoint = match llm.checkpoint {
                    Checkpoint::Dummy(_hub_repo) => false,
                    Checkpoint::Ephemeral => true,
                    Checkpoint::Hub(hub_repo) => hub_repo.repo_id.is_empty(),
                    Checkpoint::P2P(hub_repo) => hub_repo.repo_id.is_empty(),
                };

                if bad_checkpoint {
                    msg!("model check failed: bad checkpoint");
                    return false;
                }
                if !match llm.optimizer {
                    OptimizerDefinition::Dummy => false,
                    OptimizerDefinition::AdamW { .. } => true,
                    OptimizerDefinition::Distro { .. } => true,
                } {
                    msg!("model check failed: bad optimizer");
                    return false;
                }
                true
            }
        }
    }
}
