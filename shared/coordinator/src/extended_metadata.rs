use anchor_lang::prelude::borsh::{self, BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use serde_with::{Bytes, serde_as};
use ts_rs::TS;

use crate::model::{
    Checkpoint, LLM, LLMArchitecture, LLMTrainingDataLocation, LLMTrainingDataType, Model,
};
use psyche_core::{LearningRateSchedule, OptimizerDefinition};

/// Size of the extended metadata blob in bytes.
/// 2KB should be enough for JSON-encoded metadata.
pub const EXTENDED_METADATA_BYTES: usize = 2048;

/// On-chain blob for extended metadata.
/// This stores JSON-encoded metadata that the coordinator program doesn't need to parse.
/// Clients deserialize this into `ExtendedMetadataSchema`.
///
/// Uses Pod + Zeroable for zero-copy account access.
#[serde_as]
#[derive(
    Debug, Clone, Copy, Zeroable, BorshSerialize, BorshDeserialize, Serialize, Deserialize, TS,
)]
#[repr(C)]
pub struct ExtendedMetadata {
    /// Length of the actual JSON content in bytes
    pub length: u16,
    /// Raw bytes containing JSON-encoded metadata
    #[serde_as(as = "Bytes")]
    #[ts(type = "number[]")]
    pub bytes: [u8; EXTENDED_METADATA_BYTES],
}

unsafe impl Pod for ExtendedMetadata {}

impl Default for ExtendedMetadata {
    fn default() -> Self {
        Self {
            length: 0,
            bytes: [0u8; EXTENDED_METADATA_BYTES],
        }
    }
}

impl ExtendedMetadata {
    /// Create a new ExtendedMetadata from JSON bytes
    pub fn from_json(json: &[u8]) -> Result<Self, ExtendedMetadataError> {
        if json.len() > EXTENDED_METADATA_BYTES {
            return Err(ExtendedMetadataError::TooLarge {
                size: json.len(),
                max: EXTENDED_METADATA_BYTES,
            });
        }

        let mut bytes = [0u8; EXTENDED_METADATA_BYTES];
        bytes[..json.len()].copy_from_slice(json);

        Ok(Self {
            length: json.len() as u16,
            bytes,
        })
    }

    /// Get the JSON bytes
    pub fn as_json(&self) -> &[u8] {
        &self.bytes[..self.length as usize]
    }

    /// Deserialize into the schema struct
    pub fn deserialize_schema(&self) -> Result<ExtendedMetadataSchema, ExtendedMetadataError> {
        serde_json::from_slice(self.as_json()).map_err(ExtendedMetadataError::JsonParse)
    }
}

#[derive(Debug)]
pub enum ExtendedMetadataError {
    TooLarge { size: usize, max: usize },
    JsonParse(serde_json::Error),
    JsonSerialize(serde_json::Error),
}

impl std::fmt::Display for ExtendedMetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtendedMetadataError::TooLarge { size, max } => {
                write!(f, "Metadata too large: {} bytes (max {})", size, max)
            }
            ExtendedMetadataError::JsonParse(e) => write!(f, "Failed to parse JSON: {}", e),
            ExtendedMetadataError::JsonSerialize(e) => {
                write!(f, "Failed to serialize JSON: {}", e)
            }
        }
    }
}

impl std::error::Error for ExtendedMetadataError {}

// ============================================================================
// JSON Schema for client-side serialization
// ============================================================================

/// The JSON schema for extended metadata.
/// This is what clients serialize/deserialize from the on-chain blob.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtendedMetadataSchema {
    /// Schema version for forward compatibility
    #[serde(default = "default_version")]
    pub version: u32,

    /// Run metadata (name, description, etc.)
    #[serde(default)]
    pub run: RunMetadataSchema,

    /// Static model configuration (everything except checkpoint)
    #[serde(default)]
    pub model: ModelConfigSchema,

    /// Required client version for docker image resolution
    #[serde(default)]
    pub client_version: String,

    /// Optional config presets for automated joining
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_presets: Option<ConfigPresetsSchema>,

    /// Optional download authentication info
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<DownloadsSchema>,

    /// Optional lifecycle status
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<LifecycleSchema>,
}

fn default_version() -> u32 {
    1
}

impl ExtendedMetadataSchema {
    /// Serialize to JSON bytes for storing on-chain
    pub fn to_json(&self) -> Result<Vec<u8>, ExtendedMetadataError> {
        serde_json::to_vec(self).map_err(ExtendedMetadataError::JsonSerialize)
    }

    /// Create an ExtendedMetadata blob from this schema
    pub fn to_extended_metadata(&self) -> Result<ExtendedMetadata, ExtendedMetadataError> {
        let json = self.to_json()?;
        ExtendedMetadata::from_json(&json)
    }
}

/// Run metadata - display information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunMetadataSchema {
    #[serde(default)]
    pub name: String,

    #[serde(default)]
    pub description: String,

    #[serde(default)]
    pub num_parameters: u64,

    #[serde(default)]
    pub vocab_size: u64,
}

/// Static model configuration (checkpoint is stored separately on-chain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfigSchema {
    #[serde(default = "default_max_seq_len")]
    pub max_seq_len: u32,

    #[serde(default)]
    pub cold_start_warmup_steps: u32,

    #[serde(default = "default_architecture")]
    pub architecture: LLMArchitecture,

    #[serde(default = "default_data_type")]
    pub data_type: LLMTrainingDataType,

    #[serde(default)]
    pub data_location: LLMTrainingDataLocation,

    #[serde(default = "default_lr_schedule")]
    pub lr_schedule: LearningRateSchedule,

    #[serde(default = "default_optimizer")]
    pub optimizer: OptimizerDefinition,
}

fn default_max_seq_len() -> u32 {
    2048
}

fn default_architecture() -> LLMArchitecture {
    LLMArchitecture::HfLlama
}

fn default_data_type() -> LLMTrainingDataType {
    LLMTrainingDataType::Pretraining
}

fn default_lr_schedule() -> LearningRateSchedule {
    LearningRateSchedule::Constant(psyche_core::ConstantLR::default())
}

fn default_optimizer() -> OptimizerDefinition {
    OptimizerDefinition::Dummy
}

impl Default for ModelConfigSchema {
    fn default() -> Self {
        Self {
            max_seq_len: default_max_seq_len(),
            cold_start_warmup_steps: 0,
            architecture: default_architecture(),
            data_type: default_data_type(),
            data_location: LLMTrainingDataLocation::default(),
            lr_schedule: default_lr_schedule(),
            optimizer: default_optimizer(),
        }
    }
}

impl ModelConfigSchema {
    /// Combine this static model config with a dynamic checkpoint from on-chain
    /// to produce a complete LLM struct.
    ///
    /// The checkpoint must be provided separately because it's the only part
    /// of the model that changes on-chain (Hub -> P2P transitions).
    pub fn to_llm(&self, checkpoint: Checkpoint) -> LLM {
        LLM {
            max_seq_len: self.max_seq_len,
            cold_start_warmup_steps: self.cold_start_warmup_steps,
            architecture: self.architecture,
            checkpoint,
            data_type: self.data_type,
            data_location: self.data_location.clone(),
            lr_schedule: self.lr_schedule.clone(),
            optimizer: self.optimizer.clone(),
        }
    }

    /// Combine this static model config with a dynamic checkpoint from on-chain
    /// to produce a complete Model struct.
    pub fn to_model(&self, checkpoint: Checkpoint) -> Model {
        Model::LLM(self.to_llm(checkpoint))
    }
}

/// Config presets for automated run joining
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigPresetsSchema {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_micro_batch: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_total_batch: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_gpu_memory_gb: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_gpu: Option<String>,
}

/// Download authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DownloadsSchema {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_auth: Option<AuthConfigSchema>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_auth: Option<AuthConfigSchema>,
}

/// Authentication configuration for downloads
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AuthConfigSchema {
    #[serde(rename = "bearer")]
    Bearer { endpoint: String },

    #[serde(rename = "api_key")]
    ApiKey { header: String },

    #[serde(rename = "none")]
    None,
}

/// Lifecycle status information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LifecycleSchema {
    #[serde(default)]
    pub status: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub announcements: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let schema = ExtendedMetadataSchema {
            version: 1,
            run: RunMetadataSchema {
                name: "Test Run".to_string(),
                description: "A test run".to_string(),
                num_parameters: 20_000_000,
                vocab_size: 32_000,
            },
            model: ModelConfigSchema::default(),
            client_version: "v1.0.0".to_string(),
            config_presets: None,
            downloads: None,
            lifecycle: None,
        };

        let blob = schema.to_extended_metadata().unwrap();
        let parsed = blob.deserialize_schema().unwrap();

        assert_eq!(parsed.version, schema.version);
        assert_eq!(parsed.run.name, schema.run.name);
        assert_eq!(parsed.client_version, schema.client_version);
    }

    #[test]
    fn test_too_large() {
        let large_json = vec![b'x'; EXTENDED_METADATA_BYTES + 1];
        let result = ExtendedMetadata::from_json(&large_json);
        assert!(matches!(
            result,
            Err(ExtendedMetadataError::TooLarge { .. })
        ));
    }

    #[test]
    fn test_model_config_to_llm() {
        use crate::model::HubRepo;

        let model_config = ModelConfigSchema {
            max_seq_len: 4096,
            cold_start_warmup_steps: 100,
            architecture: LLMArchitecture::HfLlama,
            data_type: LLMTrainingDataType::Pretraining,
            data_location: LLMTrainingDataLocation::default(),
            lr_schedule: default_lr_schedule(),
            optimizer: default_optimizer(),
        };

        let checkpoint = Checkpoint::Hub(HubRepo {
            repo_id: psyche_core::FixedString::from_str_truncated("emozilla/llama2-20m-init"),
            revision: None,
        });

        let llm = model_config.to_llm(checkpoint);

        assert_eq!(llm.max_seq_len, 4096);
        assert_eq!(llm.cold_start_warmup_steps, 100);
        assert_eq!(llm.architecture, LLMArchitecture::HfLlama);
        assert!(matches!(llm.checkpoint, Checkpoint::Hub(_)));
    }
}
