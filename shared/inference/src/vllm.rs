//! vLLM inference bindings for Rust
//!
//! This module provides Rust FFI bindings to call Python vLLM inference engines via PyO3.
//!
//! # Architecture
//!
//! For production use in Psyche inference nodes:
//! - Each inference node runs as a long-running Python subprocess
//! - The subprocess creates ONE vLLM engine and handles many inference requests
//! - When checkpoint updates arrive, the subprocess is killed and a new one spawned
//!   with the new checkpoint path
//!
//! # API
//!
//! - `create_engine()` - Create and register a vLLM engine (called once per subprocess)
//! - `run_inference()` - Run inference on a registered engine
//! - `get_engine_stats()` - Get engine statistics
//! - `list_engines()` - List all registered engines
//! - `shutdown_engine()` - Shutdown and cleanup an engine

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;

/// Response from engine creation
#[derive(Debug, Clone)]
pub struct EngineCreationResult {
    pub success: bool,
    pub engine_id: Option<String>,
    pub error: Option<String>,
}

/// Response from inference request
#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub success: bool,
    pub request_id: Option<String>,
    pub generated_text: Option<String>,
    pub full_text: Option<String>,
    pub error: Option<String>,
}

/// Response from engine shutdown
#[derive(Debug, Clone)]
pub struct ShutdownResult {
    pub success: bool,
    pub engine_id: Option<String>,
    pub error: Option<String>,
}

/// Response from engine stats request
#[derive(Debug, Clone)]
pub struct EngineStats {
    pub success: bool,
    pub engine_id: Option<String>,
    pub model_name: Option<String>,
    pub tensor_parallel_size: Option<i64>,
    pub has_unfinished_requests: Option<bool>,
    pub error: Option<String>,
}

/// Response from list engines request
#[derive(Debug, Clone)]
pub struct EngineList {
    pub success: bool,
    pub engine_ids: Vec<String>,
    pub error: Option<String>,
}

/// Helper function to convert Python dict to Rust HashMap
fn py_dict_to_hashmap(dict: &Bound<'_, PyDict>) -> PyResult<HashMap<String, PyObject>> {
    let mut map = HashMap::new();
    for (key, value) in dict.iter() {
        let key_str: String = key.extract()?;
        map.insert(key_str, value.unbind());
    }
    Ok(map)
}

/// Helper to extract optional string from HashMap
fn get_optional_string(map: &HashMap<String, PyObject>, key: &str, py: Python) -> Option<String> {
    map.get(key).and_then(|v| v.extract(py).ok())
}

/// Helper to extract optional i64 from HashMap
fn get_optional_i64(map: &HashMap<String, PyObject>, key: &str, py: Python) -> Option<i64> {
    map.get(key).and_then(|v| v.extract(py).ok())
}

/// Helper to extract optional bool from HashMap
fn get_optional_bool(map: &HashMap<String, PyObject>, key: &str, py: Python) -> Option<bool> {
    map.get(key).and_then(|v| v.extract(py).ok())
}

/// Create a new vLLM engine
pub fn create_engine(
    py: Python,
    engine_id: &str,
    model_name: &str,
    tensor_parallel_size: Option<i32>,
    dtype: Option<&str>,
    max_model_len: Option<i32>,
    gpu_memory_utilization: Option<f64>,
) -> PyResult<EngineCreationResult> {
    let rust_bridge = py.import("psyche.vllm.rust_bridge")?;

    let kwargs = PyDict::new(py);
    kwargs.set_item("engine_id", engine_id)?;
    kwargs.set_item("model_name", model_name)?;

    if let Some(tp) = tensor_parallel_size {
        kwargs.set_item("tensor_parallel_size", tp)?;
    }

    if let Some(dt) = dtype {
        kwargs.set_item("dtype", dt)?;
    }

    if let Some(mml) = max_model_len {
        kwargs.set_item("max_model_len", mml)?;
    }

    if let Some(gmu) = gpu_memory_utilization {
        kwargs.set_item("gpu_memory_utilization", gmu)?;
    }

    let result = rust_bridge.call_method("create_engine", (), Some(&kwargs))?;
    let dict = result.downcast::<PyDict>()?;
    let map = py_dict_to_hashmap(dict)?;

    let status = get_optional_string(&map, "status", py).unwrap_or_default();
    let success = status == "success";

    Ok(EngineCreationResult {
        success,
        engine_id: get_optional_string(&map, "engine_id", py),
        error: get_optional_string(&map, "error", py),
    })
}

/// Run inference on an engine
pub fn run_inference(
    py: Python,
    engine_id: &str,
    messages: Vec<crate::protocol::ChatMessage>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_tokens: Option<i32>,
) -> PyResult<InferenceResult> {
    let rust_bridge = py.import("psyche.vllm.rust_bridge")?;

    let kwargs = PyDict::new(py);
    kwargs.set_item("engine_id", engine_id)?;

    let py_messages = pyo3::types::PyList::empty(py);
    for msg in messages {
        let py_msg = pyo3::types::PyDict::new(py);
        py_msg.set_item("role", msg.role)?;
        py_msg.set_item("content", msg.content)?;
        py_messages.append(py_msg)?;
    }
    kwargs.set_item("messages", py_messages)?;

    if let Some(temp) = temperature {
        kwargs.set_item("temperature", temp)?;
    }

    if let Some(p) = top_p {
        kwargs.set_item("top_p", p)?;
    }

    if let Some(mt) = max_tokens {
        kwargs.set_item("max_tokens", mt)?;
    }

    let result = rust_bridge.call_method("run_inference", (), Some(&kwargs))?;
    let dict = result.downcast::<PyDict>()?;
    let map = py_dict_to_hashmap(dict)?;

    let status = get_optional_string(&map, "status", py).unwrap_or_default();
    let success = status == "success";

    Ok(InferenceResult {
        success,
        request_id: get_optional_string(&map, "request_id", py),
        generated_text: get_optional_string(&map, "generated_text", py),
        full_text: get_optional_string(&map, "full_text", py),
        error: get_optional_string(&map, "error", py),
    })
}

/// Shutdown an engine
pub fn shutdown_engine(py: Python, engine_id: &str) -> PyResult<ShutdownResult> {
    let rust_bridge = py.import("psyche.vllm.rust_bridge")?;

    let result = rust_bridge.call_method1("shutdown_engine", (engine_id,))?;
    let dict = result.downcast::<PyDict>()?;
    let map = py_dict_to_hashmap(dict)?;

    let status = get_optional_string(&map, "status", py).unwrap_or_default();
    let success = status == "success";

    Ok(ShutdownResult {
        success,
        engine_id: get_optional_string(&map, "engine_id", py),
        error: get_optional_string(&map, "error", py),
    })
}

/// Get stats about an engine
pub fn get_engine_stats(py: Python, engine_id: &str) -> PyResult<EngineStats> {
    let rust_bridge = py.import("psyche.vllm.rust_bridge")?;

    let result = rust_bridge.call_method1("get_engine_stats", (engine_id,))?;
    let dict = result.downcast::<PyDict>()?;
    let map = py_dict_to_hashmap(dict)?;

    let status = get_optional_string(&map, "status", py).unwrap_or_default();
    let success = status == "success";

    Ok(EngineStats {
        success,
        engine_id: get_optional_string(&map, "engine_id", py),
        model_name: get_optional_string(&map, "model_name", py),
        tensor_parallel_size: get_optional_i64(&map, "tensor_parallel_size", py),
        has_unfinished_requests: get_optional_bool(&map, "has_unfinished_requests", py),
        error: get_optional_string(&map, "error", py),
    })
}

/// List all registered engines
pub fn list_engines(py: Python) -> PyResult<EngineList> {
    let rust_bridge = py.import("psyche.vllm.rust_bridge")?;

    let result = rust_bridge.call_method0("list_engines")?;
    let dict = result.downcast::<PyDict>()?;
    let map = py_dict_to_hashmap(dict)?;

    let status = get_optional_string(&map, "status", py).unwrap_or_default();
    let success = status == "success";

    let engine_ids = map
        .get("engine_ids")
        .and_then(|v| v.extract::<Vec<String>>(py).ok())
        .unwrap_or_default();

    Ok(EngineList {
        success,
        engine_ids,
        error: get_optional_string(&map, "error", py),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::Python;

    #[test]
    fn test_list_engines() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let result = list_engines(py);
            assert!(result.is_ok());
        });
    }
}
