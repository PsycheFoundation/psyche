use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyString};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VLLMError {
    #[error("Python exception: {0}")]
    PythonError(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl From<PyErr> for VLLMError {
    fn from(err: PyErr) -> Self {
        Python::with_gil(|py| VLLMError::PythonError(err.to_string()))
    }
}

/// Configuration for the vLLM Engine
#[derive(Debug, Clone)]
pub struct VLLMConfig {
    pub model_name: String,
    pub tensor_parallel_size: usize,
    pub gpu_memory_utilization: f64,
    pub max_model_len: Option<usize>,
}

/// Parameters for text generation (matches vLLM SamplingParams)
#[derive(Debug, Serialize, Clone)]
pub struct SamplingParams {
    pub temperature: f64,
    pub top_p: f64,
    pub max_tokens: usize,
    pub stop: Option<Vec<String>>,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 1.0,
            top_p: 1.0,
            max_tokens: 512,
            stop: None,
        }
    }
}

/// The result of a single generation step
#[derive(Debug, Deserialize)]
pub struct GenerationOutput {
    pub request_id: String,
    pub text: String,
    pub finished: bool,
}

/// The Rust wrapper around the Python `UpdatableLLMEngine`
#[derive(Clone)]
pub struct VLLMEngine {
    // We wrap the PyObject in Arc<Mutex> to allow sharing across Rust threads.
    // Note: Accessing Python still requires the GIL, which PyO3 handles.
    inner: Arc<Mutex<PyObject>>,
}

impl VLLMEngine {
    /// Initialize the Python vLLM engine
    pub fn new(config: VLLMConfig) -> Result<Self, VLLMError> {
        Python::with_gil(|py| {
            // Import the custom Python module we created in Task 1.1
            // Ensure 'python/python' is in PYTHONPATH
            let module = PyModule::import(py, "psyche.inference.engine")?;

            let engine_class = module.getattr("UpdatableLLMEngine")?;

            let instance = engine_class.call1((
                config.model_name,
                config.tensor_parallel_size,
                "auto", // dtype
                config.max_model_len,
                config.gpu_memory_utilization,
            ))?;

            Ok(Self {
                inner: Arc::new(Mutex::new(instance.into())),
            })
        })
    }

    /// Add a request to the engine queue
    pub fn add_request(&self, prompt: &str, params: SamplingParams) -> Result<String, VLLMError> {
        let params_json = serde_json::to_string(&params)
            .map_err(|e| VLLMError::SerializationError(e.to_string()))?;

        Python::with_gil(|py| {
            let engine = self.inner.lock().unwrap();
            let engine_ref = engine.as_ref(py);

            // Convert params to a Python Dict
            // We use json parsing here for simplicity and robustness across the boundary
            let json_module = PyModule::import(py, "json")?;
            let py_params = json_module.call_method1("loads", (params_json,))?;
            let py_params_dict = py_params.downcast::<PyDict>()?;

            let request_id: String = engine_ref
                .call_method1("add_request", (prompt, py_params_dict))?
                .extract()?;

            Ok(request_id)
        })
    }

    /// Execute one step of inference
    /// Returns a list of outputs for requests that have made progress
    pub fn step(&self) -> Result<Vec<GenerationOutput>, VLLMError> {
        Python::with_gil(|py| {
            let engine = self.inner.lock().unwrap();
            let engine_ref = engine.as_ref(py);

            // Call python: engine.step()
            // Note: This may block for a few milliseconds depending on GPU op
            let outputs_py = engine_ref.call_method0("step")?;
            let outputs_list = outputs_py.downcast::<PyList>()?;

            let mut results = Vec::new();

            for output in outputs_list {
                // Extract data from vLLM RequestOutput object
                // We access attributes dynamically to avoid tight coupling to vLLM versions
                let request_id: String = output.getattr("request_id")?.extract()?;
                let finished: bool = output.getattr("finished")?.extract()?;

                // Get the generated text so far (assuming single output per prompt for now)
                let outputs = output.getattr("outputs")?.downcast::<PyList>()?;
                let first_output = outputs.get_item(0)?;
                let text: String = first_output.getattr("text")?.extract()?;

                results.push(GenerationOutput {
                    request_id,
                    text,
                    finished,
                });
            }

            Ok(results)
        })
    }

    /// Check if there are pending requests
    pub fn has_unfinished_requests(&self) -> Result<bool, VLLMError> {
        Python::with_gil(|py| {
            let engine = self.inner.lock().unwrap();
            let engine_ref = engine.as_ref(py);
            let result: bool = engine_ref
                .call_method0("has_unfinished_requests")?
                .extract()?;
            Ok(result)
        })
    }

    /// Placeholder for the future Weight Update call
    pub fn update_weights(&self, _shared_mem_handle: usize) -> Result<(), VLLMError> {
        // Implementation coming
        Ok(())
    }
}
