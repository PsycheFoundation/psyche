/// vLLM integration module
///
/// This module provides Rust bindings to the Python vLLM inference engine
/// with support for live weight updates during training.
///
/// # Components
///
/// - `engine`: Basic vLLM engine wrapper for inference
/// - `updater`: High-level manager for vLLM with weight update support
/// - `types`: Shared type definitions and configuration structs
///
/// # Example Usage
///
/// ```no_run
/// use psyche_inference::vllm::{VLLMWithUpdater, VLLMWithUpdaterConfig};
/// use std::collections::HashMap;
///
/// // Create vLLM for training (direct mode)
/// let vllm = VLLMWithUpdater::for_training("meta-llama/Llama-2-7b-hf")?;
///
/// // Get engine for inference
/// let engine = vllm.get_engine()?;
///
/// // Update weights from training
/// let mut delta = HashMap::new();
/// delta.insert("model.layers.0.self_attn.q_proj.weight".to_string(), tensor);
/// vllm.update_weights(delta)?;
/// ```
pub mod engine;
pub mod types;
pub mod updater;

// Re-export commonly used types
pub use engine::{VLLMEngine, VLLMError};
pub use types::{
    GenerationOutput, SamplingParams, UpdateMode, VLLMConfig, VLLMMode, VLLMWithUpdaterConfig,
    WeightDeltaBatch,
};
pub use updater::{VLLMUpdaterError, VLLMWithUpdater, compute_weight_delta};
