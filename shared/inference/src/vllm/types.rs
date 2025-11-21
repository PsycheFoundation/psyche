use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for the vLLM Engine
#[derive(Debug, Clone)]
pub struct VLLMConfig {
    pub model_name: String,
    pub tensor_parallel_size: usize,
    pub gpu_memory_utilization: f64,
    pub max_model_len: Option<usize>,
}

impl VLLMConfig {
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            tensor_parallel_size: 1,
            gpu_memory_utilization: 0.5,
            max_model_len: None,
        }
    }

    pub fn with_tensor_parallel(mut self, size: usize) -> Self {
        self.tensor_parallel_size = size;
        self
    }

    pub fn with_gpu_memory(mut self, utilization: f64) -> Self {
        self.gpu_memory_utilization = utilization;
        self
    }

    pub fn with_max_len(mut self, max_len: usize) -> Self {
        self.max_model_len = Some(max_len);
        self
    }
}

/// Parameters for text generation (matches vLLM SamplingParams)
#[derive(Debug, Serialize, Clone)]
pub struct SamplingParams {
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: usize,
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            top_p: 1.0,
            max_tokens: 512,
            stop: None,
            seed: None,
        }
    }
}

impl SamplingParams {
    pub fn greedy() -> Self {
        Self {
            temperature: 0.0,
            ..Default::default()
        }
    }

    pub fn with_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature;
        self
    }

    pub fn with_top_p(mut self, top_p: f64) -> Self {
        self.top_p = top_p;
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
}

/// The result of a single generation step
#[derive(Debug, Deserialize, Clone)]
pub struct GenerationOutput {
    pub request_id: String,
    pub text: String,
    pub finished: bool,
}

/// Mode for VLLMWithUpdater
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VLLMMode {
    /// Direct engine API (for training/updater)
    Direct,
    /// OpenAI-compatible API server (for Atropos)
    Server,
}

/// Configuration for VLLMWithUpdater
#[derive(Debug, Clone)]
pub struct VLLMWithUpdaterConfig {
    pub model_name: String,
    pub tensor_parallel_size: usize,
    pub gpu_memory_utilization: f64,
    pub max_model_len: Option<usize>,
    pub mode: VLLMMode,
    pub server_port: Option<u16>,
    pub update_mode: UpdateMode,
}

impl VLLMWithUpdaterConfig {
    pub fn for_training(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            tensor_parallel_size: 1,
            gpu_memory_utilization: 0.5,
            max_model_len: None,
            mode: VLLMMode::Direct,
            server_port: None,
            update_mode: UpdateMode::Delta,
        }
    }

    pub fn for_atropos(model_name: impl Into<String>, port: u16) -> Self {
        Self {
            model_name: model_name.into(),
            tensor_parallel_size: 1,
            gpu_memory_utilization: 0.5,
            max_model_len: None,
            mode: VLLMMode::Server,
            server_port: Some(port),
            update_mode: UpdateMode::Delta,
        }
    }

    pub fn with_tensor_parallel(mut self, size: usize) -> Self {
        self.tensor_parallel_size = size;
        self
    }

    pub fn with_gpu_memory(mut self, utilization: f64) -> Self {
        self.gpu_memory_utilization = utilization;
        self
    }

    pub fn with_max_len(mut self, max_len: usize) -> Self {
        self.max_model_len = Some(max_len);
        self
    }
}

/// Update mode for weight updates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateMode {
    /// Delta update: w = w + Δw
    Delta,
    /// Full update: w = w_new
    Full,
}

impl UpdateMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            UpdateMode::Delta => "delta",
            UpdateMode::Full => "full",
        }
    }
}

/// Weight delta for a single parameter
#[derive(Debug, Clone)]
pub struct WeightDelta {
    pub param_name: String,
    pub delta: tch::Tensor,
}

/// Batch of weight deltas
pub type WeightDeltaBatch = HashMap<String, tch::Tensor>;
