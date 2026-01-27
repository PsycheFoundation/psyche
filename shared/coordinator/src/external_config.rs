//! External model configuration stored in GCS.
//!
//! This module provides schemas for model configuration that lives outside
//! the on-chain state. The coordinator only needs minimal fields on-chain:
//! - `checkpoint` (reads and writes for Hub↔P2P transitions)
//! - `max_seq_len` (reads for sequence length)
//! - `cold_start_warmup_steps` (reads for warmup bounds)
//!
//! Everything else is stored in GCS at `gs://{checkpoint_bucket}/config/model_config.json`
//! and fetched by clients at startup.

use serde::{Deserialize, Serialize};

use crate::model::{
    Checkpoint, GcsRepo, LLM, LLMArchitecture, LLMTrainingDataLocation, LLMTrainingDataType, Model,
};
use psyche_core::{LearningRateSchedule, OptimizerDefinition};

/// Path within the bucket where config is stored
pub const CONFIG_PREFIX: &str = "config";
/// Filename for the model config
pub const MODEL_CONFIG_FILENAME: &str = "model_config.json";

// ============================================================================
// Config-file representations (old format with all fields in [model.LLM])
// ============================================================================

/// Config-file representation of the model with all fields.
/// This allows config files to keep the old format where everything
/// is under `[model.LLM]`.
///
/// Use `ConfigModel::split()` to separate into on-chain `Model` and `ExternalModelConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigModel {
    LLM(ConfigLLM),
}

impl ConfigModel {
    /// Split the config model into on-chain Model and ExternalModelConfig.
    pub fn split(self) -> (Model, ExternalModelConfig) {
        match self {
            ConfigModel::LLM(config_llm) => config_llm.split(),
        }
    }
}

/// Config-file representation of LLM with all fields (old format).
/// This includes both on-chain fields and external config fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigLLM {
    // On-chain fields
    pub max_seq_len: u32,
    #[serde(default)]
    pub cold_start_warmup_steps: u32,
    pub checkpoint: Checkpoint,

    // External config fields (with defaults for backward compatibility)
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

impl ConfigLLM {
    /// Split into on-chain LLM and ExternalModelConfig.
    pub fn split(self) -> (Model, ExternalModelConfig) {
        let llm = LLM {
            max_seq_len: self.max_seq_len,
            cold_start_warmup_steps: self.cold_start_warmup_steps,
            checkpoint: self.checkpoint,
        };

        let external_config = ExternalModelConfig {
            version: default_version(),
            architecture: self.architecture,
            data_type: self.data_type,
            data_location: self.data_location,
            lr_schedule: self.lr_schedule,
            optimizer: self.optimizer,
            run_metadata: None,
            client_requirements: None,
        };

        (Model::LLM(llm), external_config)
    }
}

/// External model configuration schema.
/// This is stored in GCS and fetched by clients.
///
/// Adding new fields here doesn't affect on-chain memory layout.
/// Use `#[serde(default)]` for backward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalModelConfig {
    /// Schema version for forward compatibility
    #[serde(default = "default_version")]
    pub version: u32,

    /// Model architecture (HfLlama, HfDeepseek, etc.)
    #[serde(default = "default_architecture")]
    pub architecture: LLMArchitecture,

    /// Training data type (Pretraining, Finetuning)
    #[serde(default = "default_data_type")]
    pub data_type: LLMTrainingDataType,

    /// Training data location
    #[serde(default)]
    pub data_location: LLMTrainingDataLocation,

    /// Learning rate schedule
    #[serde(default = "default_lr_schedule")]
    pub lr_schedule: LearningRateSchedule,

    /// Optimizer configuration
    #[serde(default = "default_optimizer")]
    pub optimizer: OptimizerDefinition,

    /// Optional run metadata
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_metadata: Option<RunMetadata>,

    /// Optional client requirements
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_requirements: Option<ClientRequirements>,
}

fn default_version() -> u32 {
    1
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

impl Default for ExternalModelConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            architecture: LLMArchitecture::HfLlama,
            data_type: LLMTrainingDataType::Pretraining,
            data_location: LLMTrainingDataLocation::default(),
            lr_schedule: default_lr_schedule(),
            optimizer: default_optimizer(),
            run_metadata: None,
            client_requirements: None,
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

/// Client requirements for joining the run
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientRequirements {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_gpu_memory_gb: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_gpu: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_micro_batch: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_total_batch: Option<u32>,
}

impl ExternalModelConfig {
    /// Serialize to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Validate the configuration
    pub fn check(&self) -> bool {
        // Validate data location
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

        // Validate optimizer
        match &self.optimizer {
            OptimizerDefinition::Dummy => false,
            OptimizerDefinition::AdamW { .. } => true,
            OptimizerDefinition::Distro { .. } => true,
        }
    }
}

/// Helper to derive the config GCS path from a checkpoint.
/// Returns `Some((bucket, path))` for GCS checkpoints, `None` for others.
pub fn get_config_gcs_path(checkpoint: &Checkpoint) -> Option<(String, String)> {
    let gcs_repo = match checkpoint {
        Checkpoint::Gcs(repo) | Checkpoint::P2PGcs(repo) => repo,
        _ => return None,
    };

    let bucket = gcs_repo.bucket.to_string();
    let path = format!("{}/{}", CONFIG_PREFIX, MODEL_CONFIG_FILENAME);

    Some((bucket, path))
}

/// Helper to derive the config Hub path from a checkpoint.
/// Returns `Some((repo_id, revision, filename))` for Hub checkpoints, `None` for others.
pub fn get_config_hub_path(
    checkpoint: &Checkpoint,
) -> Option<(String, Option<String>, &'static str)> {
    let hub_repo = match checkpoint {
        Checkpoint::Hub(repo) | Checkpoint::P2P(repo) | Checkpoint::Dummy(repo) => repo,
        _ => return None,
    };

    let repo_id = hub_repo.repo_id.to_string();
    if repo_id.is_empty() {
        return None;
    }

    let revision = hub_repo.revision.as_ref().map(|r| r.to_string());

    Some((repo_id, revision, MODEL_CONFIG_FILENAME))
}

/// Construct the full GCS URI for the config file
pub fn get_config_gcs_uri(checkpoint: &Checkpoint) -> Option<String> {
    get_config_gcs_path(checkpoint).map(|(bucket, path)| format!("gs://{}/{}", bucket, path))
}

/// Helper to create a GcsRepo for the config location from a checkpoint
pub fn get_config_gcs_repo(checkpoint: &Checkpoint) -> Option<GcsRepo> {
    match checkpoint {
        Checkpoint::Gcs(repo) | Checkpoint::P2PGcs(repo) => Some(GcsRepo {
            bucket: repo.bucket.clone(),
            prefix: Some(psyche_core::FixedString::from_str_truncated(CONFIG_PREFIX)),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psyche_core::FixedString;

    #[test]
    fn test_roundtrip() {
        let config = ExternalModelConfig {
            version: 1,
            architecture: LLMArchitecture::HfLlama,
            data_type: LLMTrainingDataType::Pretraining,
            data_location: LLMTrainingDataLocation::default(),
            lr_schedule: default_lr_schedule(),
            optimizer: OptimizerDefinition::AdamW {
                betas: [0.9, 0.999],
                weight_decay: 0.01,
                eps: 1e-8,
                clip_grad_norm: None,
            },
            run_metadata: Some(RunMetadata {
                name: "Test Run".to_string(),
                description: "A test training run".to_string(),
                num_parameters: 20_000_000,
                vocab_size: 32_000,
                client_version: "v1.0.0".to_string(),
            }),
            client_requirements: None,
        };

        let json = config.to_json().unwrap();
        let parsed = ExternalModelConfig::from_json(&json).unwrap();

        assert_eq!(parsed.version, config.version);
        assert_eq!(parsed.architecture, config.architecture);
        assert_eq!(parsed.run_metadata.unwrap().name, "Test Run");
    }

    #[test]
    fn test_backward_compatibility() {
        // Old JSON without new fields
        let old_json = r#"{
            "version": 1,
            "architecture": "HfLlama"
        }"#;

        let config = ExternalModelConfig::from_json(old_json).unwrap();

        // Should use defaults for missing fields
        assert_eq!(config.architecture, LLMArchitecture::HfLlama);
        assert!(matches!(
            config.data_location,
            LLMTrainingDataLocation::Dummy
        ));
        assert!(config.run_metadata.is_none());
    }

    #[test]
    fn test_config_gcs_path() {
        let checkpoint = Checkpoint::Gcs(GcsRepo {
            bucket: FixedString::from_str_truncated("my-bucket"),
            prefix: Some(FixedString::from_str_truncated("checkpoints")),
        });

        let (bucket, path) = get_config_gcs_path(&checkpoint).unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(path, "config/model_config.json");

        let uri = get_config_gcs_uri(&checkpoint).unwrap();
        assert_eq!(uri, "gs://my-bucket/config/model_config.json");
    }

    #[test]
    fn test_config_gcs_path_hub_returns_none() {
        use crate::model::HubRepo;

        let checkpoint = Checkpoint::Hub(HubRepo {
            repo_id: FixedString::from_str_truncated("org/model"),
            revision: None,
        });

        assert!(get_config_gcs_path(&checkpoint).is_none());
    }

    #[test]
    fn test_adding_new_fields() {
        // This test demonstrates that adding new fields doesn't break parsing
        // of old configs (as long as they have #[serde(default)])
        let config_with_future_fields = r#"{
            "version": 2,
            "architecture": "HfLlama",
            "some_future_field": "this field doesn't exist yet",
            "another_future_field": { "nested": true }
        }"#;

        // Should parse without error, ignoring unknown fields
        let config = ExternalModelConfig::from_json(config_with_future_fields).unwrap();
        assert_eq!(config.version, 2);
        assert_eq!(config.architecture, LLMArchitecture::HfLlama);
    }
}
