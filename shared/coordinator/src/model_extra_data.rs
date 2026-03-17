use anchor_lang::prelude::borsh;
use psyche_core::{FixedVec, LearningRateSchedule, OptimizerDefinition};
use serde::{Deserialize, Serialize};

use crate::model::{LLMArchitecture, LLMTrainingDataLocation, LLMTrainingDataType};

/// Path within the bucket where config is stored
pub const CONFIG_PREFIX: &str = "config";
/// Filename for the model config
pub const MODEL_CONFIG_FILENAME: &str = "model_config.json";

/// Extra model data that is stored off-chain and fetched by clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelExtraData {
    #[serde(default)]
    pub version: u32,

    pub architecture: LLMArchitecture,

    pub data_type: LLMTrainingDataType,

    pub data_location: LLMTrainingDataLocation,

    pub lr_schedule: LearningRateSchedule,

    pub optimizer: OptimizerDefinition,

    /// Optional run metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_metadata: Option<RunMetadata>,

    /// Checkpoint configuration (Hub repo, GCS bucket, Dummy, etc.)
    /// When present in a config file, this is used to populate the on-chain
    /// `LLM.checkpoint_data` opaque bytes.
    pub checkpoint: CheckpointData,
}

impl Default for ModelExtraData {
    fn default() -> Self {
        Self {
            version: 1,
            architecture: LLMArchitecture::HfLlama,
            data_type: LLMTrainingDataType::Pretraining,
            data_location: LLMTrainingDataLocation::default(),
            lr_schedule: LearningRateSchedule::Constant(psyche_core::ConstantLR::default()),
            optimizer: OptimizerDefinition::Dummy,
            run_metadata: None,
            checkpoint: CheckpointData::default(),
        }
    }
}

/// Run metadata - display information about the run
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunMetadata {
    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub description: String,

    #[serde(default)]
    pub num_parameters: u64,

    #[serde(default)]
    pub vocab_size: u64,

    #[serde(default)]
    pub client_version: String,
}

impl ModelExtraData {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Validate the configuration
    pub fn check(&self) -> bool {
        let bad_data_location = match &self.data_location {
            LLMTrainingDataLocation::Dummy => false,
            LLMTrainingDataLocation::Server(url) => url.is_empty(),
            LLMTrainingDataLocation::Local(_) => false,
            LLMTrainingDataLocation::Http(http_loc) => {
                use crate::model::HttpTrainingDataLocation;
                match &http_loc.location {
                    HttpTrainingDataLocation::SingleUrl(url) => url.is_empty(),
                    HttpTrainingDataLocation::NumberedFiles {
                        url_template,
                        num_files,
                        ..
                    } => url_template.is_empty() || *num_files == 0,
                    HttpTrainingDataLocation::Gcp { bucket_name, .. } => bucket_name.is_empty(),
                }
            }
            LLMTrainingDataLocation::WeightedHttp(url) => url.is_empty(),
            LLMTrainingDataLocation::Preprocessed(url) => url.is_empty(),
        };

        if bad_data_location {
            return false;
        }

        match &self.optimizer {
            OptimizerDefinition::Dummy => false,
            OptimizerDefinition::AdamW { .. } => true,
            OptimizerDefinition::Distro { .. } => true,
        }
    }
}

/// Off-chain checkpoint data that gets serialized into opaque bytes for on-chain storage.
/// This decouples the on-chain account layout from storage backend details.
#[derive(
    Debug, Clone, Serialize, Deserialize, borsh::BorshSerialize, borsh::BorshDeserialize, Default,
)]
pub enum CheckpointData {
    #[default]
    Dummy,
    Hub {
        repo_id: String,
        revision: Option<String>,
    },
    Gcs {
        bucket: String,
        prefix: Option<String>,
    },
}

pub const CHECKPOINT_DATA_MAX_LEN: usize = 256;

impl CheckpointData {
    pub fn to_fixed_vec(&self) -> FixedVec<u8, CHECKPOINT_DATA_MAX_LEN> {
        let bytes =
            borsh::to_vec(self).expect("CheckpointData borsh serialization should not fail");
        let mut fv = FixedVec::new();
        for b in bytes {
            fv.push(b)
                .expect("CheckpointData serialized size exceeds CHECKPOINT_DATA_MAX_LEN");
        }
        fv
    }

    pub fn from_fixed_vec(
        fv: &FixedVec<u8, CHECKPOINT_DATA_MAX_LEN>,
    ) -> Result<Self, std::io::Error> {
        borsh::BorshDeserialize::try_from_slice(&fv[..])
    }
}
