//! Inference Node implementation

use crate::protocol::{InferenceRequest, InferenceResponse};
use crate::vllm;
use anyhow::{Context, Result, anyhow};
use pyo3::prelude::*;
use tracing::{debug, info, warn};

#[derive(Debug)]
pub struct InferenceNode {
    engine_id: String,
    model_name: String,
    initialized: bool,
}

impl InferenceNode {
    pub fn new(
        model_name: String,
        _tensor_parallel_size: Option<usize>,
        _gpu_memory_utilization: Option<f64>,
    ) -> Self {
        let engine_id = format!("inference_node_{}", uuid::Uuid::new_v4());

        Self {
            engine_id,
            model_name,
            initialized: false,
        }
    }

    /// Initialize the vLLM engine
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
            if !result.success {
                let error = result.error.unwrap_or_else(|| "Unknown error".to_string());
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
            "Running inference for request: {} with {} messages",
            request.request_id,
            request.messages.len()
        );

        Python::with_gil(|py| {
            let result = vllm::run_inference(
                py,
                &self.engine_id,
                request.messages.clone(),
                Some(request.temperature),
                Some(request.top_p),
                Some(request.max_tokens as i32),
            )
            .context("Failed to run inference")?;

            // Check status
            if !result.success {
                let error = result.error.unwrap_or_else(|| "Unknown error".to_string());
                return Err(anyhow!("Inference failed: {}", error));
            }

            // Extract generated text
            let generated_text = result
                .generated_text
                .ok_or_else(|| anyhow!("Missing generated_text in response"))?;

            let full_text = result
                .full_text
                .ok_or_else(|| anyhow!("Missing full_text in response"))?;

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
    pub fn shutdown(&mut self) -> Result<()> {
        if !self.initialized {
            return Ok(());
        }

        info!("Shutting down inference node: {}", self.engine_id);

        Python::with_gil(|py| {
            vllm::shutdown_engine(py, &self.engine_id).context("Failed to shutdown engine")?;
            self.initialized = false; // Mark as shutdown to prevent double-shutdown
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
    use crate::ChatMessage;

    #[test]
    fn test_node_creation() {
        let node = InferenceNode::new("gpt2".to_string(), Some(1), Some(0.3));
        assert_eq!(node.model_name(), "gpt2");
        assert!(!node.initialized);
    }

    #[test]
    fn test_node_engine_id_uniqueness() {
        let node1 = InferenceNode::new("gpt2".to_string(), None, None);
        let node2 = InferenceNode::new("gpt2".to_string(), None, None);

        // Each node should have a unique engine ID
        assert_ne!(node1.engine_id(), node2.engine_id());
        assert!(node1.engine_id().starts_with("inference_node_"));
        assert!(node2.engine_id().starts_with("inference_node_"));
    }

    #[test]
    fn test_inference_before_initialization() {
        let node = InferenceNode::new("gpt2".to_string(), None, None);

        let request = InferenceRequest {
            request_id: "test-1".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            max_tokens: 10,
            temperature: 0.7,
            top_p: 0.9,
            stream: false,
        };

        // Should fail because engine is not initialized
        let result = node.inference(&request);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not initialized"));
    }
}
