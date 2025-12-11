//! Inference Node implementation
//!
//! An inference node loads a model via vLLM and serves inference requests.

use crate::protocol::{InferenceRequest, InferenceResponse};
use crate::vllm;
use anyhow::{Context, Result, anyhow};
use pyo3::prelude::*;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Inference node that serves a model via vLLM
pub struct InferenceNode {
    /// Unique engine ID
    engine_id: String,

    /// Model name/path
    model_name: String,

    /// Whether the engine is initialized
    initialized: bool,
}

impl InferenceNode {
    /// Create a new inference node
    ///
    /// # Arguments
    /// * `model_name` - Model name or path (e.g., "gpt2", "/path/to/model")
    /// * `tensor_parallel_size` - Number of GPUs for tensor parallelism
    /// * `gpu_memory_utilization` - Fraction of GPU memory to use (0.0-1.0)
    pub fn new(
        model_name: String,
        tensor_parallel_size: Option<usize>,
        gpu_memory_utilization: Option<f64>,
    ) -> Self {
        let engine_id = format!("inference_node_{}", uuid::Uuid::new_v4());

        Self {
            engine_id,
            model_name,
            initialized: false,
        }
    }

    /// Initialize the vLLM engine
    ///
    /// This must be called before running inference.
    pub fn initialize(
        &mut self,
        tensor_parallel_size: Option<usize>,
        gpu_memory_utilization: Option<f64>,
    ) -> Result<()> {
        if self.initialized {
            warn!("Engine already initialized, skipping");
            return Ok(());
        }

        info!(
            "Initializing inference node with model: {}",
            self.model_name
        );

        Python::with_gil(|py| {
            let result = vllm::create_engine(
                py,
                &self.engine_id,
                &self.model_name,
                tensor_parallel_size.map(|x| x as i32),
                Some("auto"),
                None, // max_model_len
                gpu_memory_utilization,
            )
            .context("Failed to create vLLM engine")?;

            // Check status
            let status: String = result
                .get("status")
                .ok_or_else(|| anyhow!("Missing status in response"))?
                .extract(py)
                .context("Failed to extract status")?;

            if status != "success" {
                let error = result
                    .get("error")
                    .and_then(|e| e.extract::<String>(py).ok())
                    .unwrap_or_else(|| "Unknown error".to_string());
                return Err(anyhow!("Engine creation failed: {}", error));
            }

            info!("vLLM engine initialized successfully: {}", self.engine_id);
            self.initialized = true;
            Ok(())
        })
    }

    /// Run inference on a request
    pub fn inference(&self, request: &InferenceRequest) -> Result<InferenceResponse> {
        if !self.initialized {
            return Err(anyhow!("Engine not initialized. Call initialize() first."));
        }

        debug!(
            "Running inference for request: {} with prompt: {:?}",
            request.request_id,
            request.prompt.chars().take(50).collect::<String>()
        );

        Python::with_gil(|py| {
            let result = vllm::run_inference(
                py,
                &self.engine_id,
                &request.prompt,
                Some(request.temperature),
                Some(request.top_p),
                Some(request.max_tokens as i32),
            )
            .context("Failed to run inference")?;

            // Check status
            let status: String = result
                .get("status")
                .ok_or_else(|| anyhow!("Missing status in response"))?
                .extract(py)
                .context("Failed to extract status")?;

            if status != "success" {
                let error = result
                    .get("error")
                    .and_then(|e| e.extract::<String>(py).ok())
                    .unwrap_or_else(|| "Unknown error".to_string());
                return Err(anyhow!("Inference failed: {}", error));
            }

            // Extract generated text
            let generated_text: String = result
                .get("generated_text")
                .ok_or_else(|| anyhow!("Missing generated_text in response"))?
                .extract(py)
                .context("Failed to extract generated_text")?;

            let full_text: String = result
                .get("full_text")
                .ok_or_else(|| anyhow!("Missing full_text in response"))?
                .extract(py)
                .context("Failed to extract full_text")?;

            debug!(
                "Inference completed for request: {}, generated {} chars",
                request.request_id,
                generated_text.len()
            );

            Ok(InferenceResponse {
                request_id: request.request_id.clone(),
                generated_text,
                full_text,
                finish_reason: Some("stop".to_string()),
            })
        })
    }

    /// Shutdown the engine and cleanup resources
    pub fn shutdown(&self) -> Result<()> {
        if !self.initialized {
            return Ok(());
        }

        info!("Shutting down inference node: {}", self.engine_id);

        Python::with_gil(|py| {
            vllm::shutdown_engine(py, &self.engine_id).context("Failed to shutdown engine")?;
            Ok(())
        })
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Get the engine ID
    pub fn engine_id(&self) -> &str {
        &self.engine_id
    }
}

impl Drop for InferenceNode {
    fn drop(&mut self) {
        if self.initialized {
            if let Err(e) = self.shutdown() {
                warn!("Failed to shutdown engine in Drop: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let node = InferenceNode::new("gpt2".to_string(), Some(1), Some(0.3));
        assert_eq!(node.model_name(), "gpt2");
        assert!(!node.initialized);
    }
}
