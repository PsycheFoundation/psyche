use psyche_coordinator::model::{LLMArchitecture, Model};
use psyche_core::{LearningRateSchedule, OptimizerDefinition};
use psyche_modeling::{AttentionImplementation, Devices};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct StateConfig {
    pub config: PartialCoordinatorConfig,
    pub model: Model,
}

#[derive(Deserialize)]
pub struct PartialCoordinatorConfig {
    pub total_steps: u32,
    #[serde(default = "default_batch_size")]
    pub global_batch_size_end: u16,
}

// serde is silly and requires a fn for a default
fn default_batch_size() -> u16 {
    256
}

pub struct TrainParams {
    pub model: String,
    pub sequence_length: usize,
    pub optimizer: OptimizerDefinition,
    pub lr_schedule: LearningRateSchedule,
    pub total_steps: u32,
    pub total_batch: usize,
    pub micro_batch: usize,
    pub device: Devices,
    pub grad_accum_in_fp32: bool,
    pub tensor_parallelism: Option<usize>,
    pub data_parallelism: Option<usize>,
    pub attn_implementation: Option<AttentionImplementation>,
    pub start_step: u32,
    pub architecture: LLMArchitecture,
    pub save_path: Option<String>,
}
