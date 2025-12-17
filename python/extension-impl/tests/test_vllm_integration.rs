// Integration test for vLLM Rust bindings

#[cfg(feature = "vllm-tests")]
use psyche_python_extension_impl::vllm;
#[cfg(feature = "vllm-tests")]
use pyo3::Python;

#[test]
#[cfg(feature = "vllm-tests")]
fn test_create_and_shutdown_engine() {
    // Initialize Python
    pyo3::prepare_freethreaded_python();

    Python::with_gil(|py| {
        // Create engine
        let result = vllm::create_engine(
            py,
            "test_engine",
            "gpt2",
            Some(1),      // tensor_parallel_size
            Some("auto"), // dtype
            Some(512),    // max_model_len
            Some(0.3),    // gpu_memory_utilization
        );

        assert!(
            result.is_ok(),
            "Failed to create engine: {:?}",
            result.err()
        );

        let response = result.unwrap();
        assert!(response.success, "Engine creation failed");
        assert_eq!(response.engine_id.as_deref(), Some("test_engine"));

        // Shutdown engine
        let result = vllm::shutdown_engine(py, "test_engine");
        assert!(
            result.is_ok(),
            "Failed to shutdown engine: {:?}",
            result.err()
        );

        let shutdown_response = result.unwrap();
        assert!(shutdown_response.success, "Engine shutdown failed");
    });
}

#[test]
#[cfg(feature = "vllm-tests")]
fn test_list_engines() {
    pyo3::prepare_freethreaded_python();

    Python::with_gil(|py| {
        let result = vllm::list_engines(py);
        assert!(result.is_ok(), "Failed to list engines: {:?}", result.err());

        let response = result.unwrap();
        assert!(response.success, "List engines failed");
    });
}
