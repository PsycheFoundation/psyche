use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

use super::engine::{VLLMEngine, VLLMError};
use super::types::{UpdateMode, VLLMMode, VLLMWithUpdaterConfig, WeightDeltaBatch};

#[derive(Error, Debug)]
pub enum VLLMUpdaterError {
    #[error("Python exception: {0}")]
    PythonError(String),
    #[error("VLLMError: {0}")]
    VLLMError(#[from] VLLMError),
    #[error("Server mode not yet supported for weight updates")]
    ServerModeNotSupported,
    #[error("Weight queue not initialized")]
    QueueNotInitialized,
}

impl From<PyErr> for VLLMUpdaterError {
    fn from(err: PyErr) -> Self {
        Python::with_gil(|py| VLLMUpdaterError::PythonError(err.to_string()))
    }
}

/// Rust wrapper around Python's VLLMWithUpdater
///
/// This provides a high-level interface for vLLM with live weight updates.
/// Supports two modes:
/// - Direct: Direct engine API (for training)
/// - Server: OpenAI-compatible API server (for Atropos)
pub struct VLLMWithUpdater {
    py_manager: Arc<Mutex<PyObject>>,
    config: VLLMWithUpdaterConfig,
}

impl VLLMWithUpdater {
    /// Initialize VLLMWithUpdater from config
    ///
    /// # Example
    /// ```no_run
    /// use psyche_inference::vllm::{VLLMWithUpdater, VLLMWithUpdaterConfig};
    ///
    /// let config = VLLMWithUpdaterConfig::for_training("meta-llama/Llama-2-7b-hf");
    /// let vllm = VLLMWithUpdater::new(config)?;
    /// ```
    pub fn new(config: VLLMWithUpdaterConfig) -> Result<Self, VLLMUpdaterError> {
        Python::with_gil(|py| {
            let module = PyModule::import(py, "psyche.vllm.manager")?;
            let manager_cls = module.getattr("VLLMWithUpdater")?;

            // Convert config to Python kwargs
            let kwargs = PyDict::new(py);
            kwargs.set_item("model_name", &config.model_name)?;
            kwargs.set_item("tensor_parallel_size", config.tensor_parallel_size)?;
            kwargs.set_item("gpu_memory_utilization", config.gpu_memory_utilization)?;

            if let Some(max_len) = config.max_model_len {
                kwargs.set_item("max_model_len", max_len)?;
            }

            kwargs.set_item(
                "mode",
                match config.mode {
                    VLLMMode::Direct => "direct",
                    VLLMMode::Server => "server",
                },
            )?;

            if let Some(port) = config.server_port {
                kwargs.set_item("server_port", port)?;
            }

            kwargs.set_item("update_mode", config.update_mode.as_str())?;

            // Create manager instance
            let manager = manager_cls.call((), Some(kwargs))?;

            Ok(Self {
                py_manager: Arc::new(Mutex::new(manager.into())),
                config,
            })
        })
    }

    /// Convenience constructor for training use (direct mode)
    pub fn for_training(model_name: impl Into<String>) -> Result<Self, VLLMUpdaterError> {
        Self::new(VLLMWithUpdaterConfig::for_training(model_name))
    }

    /// Convenience constructor for Atropos use (server mode)
    pub fn for_atropos(model_name: impl Into<String>, port: u16) -> Result<Self, VLLMUpdaterError> {
        Self::new(VLLMWithUpdaterConfig::for_atropos(model_name, port))
    }

    /// Get the underlying VLLMEngine (only available in direct mode)
    ///
    /// This allows direct inference calls via the engine API.
    pub fn get_engine(&self) -> Result<VLLMEngine, VLLMUpdaterError> {
        if self.config.mode != VLLMMode::Direct {
            return Err(VLLMUpdaterError::ServerModeNotSupported);
        }

        Python::with_gil(|py| {
            let manager = self.py_manager.lock().unwrap();
            let py_engine = manager.getattr(py, "engine")?;

            Ok(VLLMEngine {
                inner: Arc::new(Mutex::new(py_engine)),
            })
        })
    }

    /// Update vLLM weights with delta from training
    ///
    /// # Arguments
    /// * `weight_delta` - HashMap of parameter names to delta tensors
    ///
    /// # Example
    /// ```no_run
    /// use std::collections::HashMap;
    /// use tch::Tensor;
    ///
    /// let mut delta = HashMap::new();
    /// delta.insert(
    ///     "model.layers.0.self_attn.q_proj.weight".to_string(),
    ///     Tensor::randn(&[4096, 4096], tch::kind::FLOAT_CPU),
    /// );
    ///
    /// vllm.update_weights(delta)?;
    /// ```
    pub fn update_weights(&self, weight_delta: WeightDeltaBatch) -> Result<(), VLLMUpdaterError> {
        if self.config.mode == VLLMMode::Server {
            return Err(VLLMUpdaterError::ServerModeNotSupported);
        }

        Python::with_gil(|py| {
            // Convert HashMap<String, Tensor> to Python dict
            let py_dict = PyDict::new(py);

            for (name, tensor) in weight_delta {
                // Convert tch::Tensor to PyTorch tensor via PyO3
                let py_tensor = tensor.shallow_clone().into_py(py);
                py_dict.set_item(name, py_tensor)?;
            }

            let manager = self.py_manager.lock().unwrap();
            manager.call_method1(py, "update_weights", (py_dict,))?;

            Ok(())
        })
    }

    /// Signal updater to checkpoint current state
    ///
    /// This creates a snapshot of current weights for error recovery.
    pub fn checkpoint(&self) -> Result<(), VLLMUpdaterError> {
        if self.config.mode == VLLMMode::Server {
            return Err(VLLMUpdaterError::ServerModeNotSupported);
        }

        Python::with_gil(|py| {
            let manager = self.py_manager.lock().unwrap();
            manager.call_method0(py, "checkpoint")?;
            Ok(())
        })
    }

    /// Clean shutdown of all components
    pub fn shutdown(&self) -> Result<(), VLLMUpdaterError> {
        Python::with_gil(|py| {
            let manager = self.py_manager.lock().unwrap();
            manager.call_method0(py, "shutdown")?;
            Ok(())
        })
    }

    /// Get server URL (only in server mode)
    pub fn server_url(&self) -> Option<String> {
        if self.config.mode == VLLMMode::Server {
            self.config
                .server_port
                .map(|port| format!("http://localhost:{}", port))
        } else {
            None
        }
    }

    /// Get the mode this instance is running in
    pub fn mode(&self) -> VLLMMode {
        self.config.mode
    }
}

impl Drop for VLLMWithUpdater {
    fn drop(&mut self) {
        // Best-effort shutdown, ignore errors
        let _ = self.shutdown();
    }
}

/// Helper function to compute weight delta between current and reference model
///
/// This computes Δw = w_current - w_reference for all parameters.
///
/// # Example
/// ```no_run
/// use psyche_inference::vllm::compute_weight_delta;
///
/// // After training step
/// let delta = compute_weight_delta(&current_model, &reference_model)?;
/// vllm.update_weights(delta)?;
/// ```
pub fn compute_weight_delta(
    current_model: &PyObject,
    reference_model: &PyObject,
) -> Result<WeightDeltaBatch, VLLMUpdaterError> {
    Python::with_gil(|py| {
        // Import the utility function from Python
        let utils_module = PyModule::import(py, "psyche.rl.utils")?;
        let compute_fn = utils_module.getattr("compute_weight_delta")?;

        // Call Python function
        let py_result = compute_fn.call1((current_model, reference_model))?;

        // Convert result to Rust HashMap
        let py_dict = py_result.downcast::<PyDict>()?;
        let mut delta = HashMap::new();

        for (key, value) in py_dict.iter() {
            let param_name: String = key.extract()?;
            let tensor: tch::Tensor = value.extract()?;
            delta.insert(param_name, tensor);
        }

        Ok(delta)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires vLLM installed
    fn test_create_for_training() {
        let result = VLLMWithUpdater::for_training("gpt2");
        assert!(result.is_ok());
    }

    #[test]
    #[ignore] // Requires vLLM installed
    fn test_create_for_atropos() {
        let result = VLLMWithUpdater::for_atropos("gpt2", 9001);
        assert!(result.is_ok());
    }
}
