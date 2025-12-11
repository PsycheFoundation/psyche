// Integration test for vLLM Rust bindings

use psyche_inference::vllm;
use pyo3::Python;

#[test]
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
        assert!(response.contains_key("status"));

        let status: String = response.get("status").unwrap().extract(py).unwrap();
        assert_eq!(status, "success");

        // Shutdown engine
        let result = vllm::shutdown_engine(py, "test_engine");
        assert!(
            result.is_ok(),
            "Failed to shutdown engine: {:?}",
            result.err()
        );
    });
}

#[test]
fn test_list_engines() {
    pyo3::prepare_freethreaded_python();

    Python::with_gil(|py| {
        let result = vllm::list_engines(py);
        assert!(result.is_ok(), "Failed to list engines: {:?}", result.err());

        let response = result.unwrap();
        assert!(response.contains_key("status"));

        let status: String = response.get("status").unwrap().extract(py).unwrap();
        assert_eq!(status, "success");
    });
}

#[test]
fn test_run_inference() {
    // This test requires vLLM to be installed
    // Skip if not available
    pyo3::prepare_freethreaded_python();

    Python::with_gil(|py| {
        // Check if vLLM is available
        let check = py.import("psyche.vllm.rust_bridge");
        if check.is_err() {
            println!("Skipping test: vLLM not available");
            return;
        }

        // Create engine
        let result = vllm::create_engine(
            py,
            "inference_test",
            "gpt2",
            Some(1),
            Some("auto"),
            Some(512),
            Some(0.3),
        );

        if result.is_err() {
            println!("Skipping test: Failed to create engine");
            return;
        }

        // Run inference
        let result = vllm::run_inference(
            py,
            "inference_test",
            "Once upon a time",
            Some(0.7), // temperature
            Some(0.9), // top_p
            Some(20),  // max_tokens
        );

        assert!(result.is_ok(), "Inference failed: {:?}", result.err());

        let response = result.unwrap();
        assert!(response.contains_key("status"));

        let status: String = response.get("status").unwrap().extract(py).unwrap();
        assert_eq!(status, "success");

        assert!(response.contains_key("generated_text"));

        // Cleanup
        let _ = vllm::shutdown_engine(py, "inference_test");
    });
}
