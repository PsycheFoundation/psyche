// Integration test for InferenceNode

#[cfg(feature = "vllm-tests")]
use psyche_inference::node::InferenceNode;
#[cfg(feature = "vllm-tests")]
use psyche_inference::protocol::InferenceRequest;
#[cfg(feature = "vllm-tests")]
use serial_test::serial;

#[test]
#[serial]
#[cfg(feature = "vllm-tests")]
fn test_inference_node_local() {
    pyo3::prepare_freethreaded_python();

    pyo3::Python::with_gil(|py| {
        let check = py.import("psyche.vllm.rust_bridge");
        if check.is_err() {
            println!("Skipping test: vLLM not available");
            return;
        }

        let mut node = InferenceNode::new(
            "gpt2".to_string(),
            Some(1),   // tensor_parallel_size
            Some(0.3), // gpu_memory_utilization - low for testing
        );

        let init_result = node.initialize(Some(1), Some(0.3));
        if init_result.is_err() {
            println!("Skipping test: Failed to initialize vLLM engine");
            println!("Error: {:?}", init_result.err());
            return;
        }

        let request = InferenceRequest {
            request_id: "test-request-1".to_string(),
            prompt: "Once upon a time".to_string(),
            max_tokens: 20,
            temperature: 0.7,
            top_p: 0.9,
            stream: false,
        };

        // Run inference
        let result = node.inference(&request);
        assert!(result.is_ok(), "Inference failed: {:?}", result.err());

        let response = result.unwrap();
        assert_eq!(response.request_id, "test-request-1");
        assert!(
            !response.generated_text.is_empty(),
            "Generated text is empty"
        );
        assert!(
            response.full_text.starts_with("Once upon a time"),
            "Full text doesn't start with prompt"
        );

        println!("Generated: {}", response.generated_text);

        // Cleanup
        let _ = node.shutdown();
    });
}

#[test]
#[serial]
#[cfg(feature = "vllm-tests")]
fn test_inference_node_multiple_requests() {
    pyo3::prepare_freethreaded_python();

    pyo3::Python::with_gil(|py| {
        let check = py.import("psyche.vllm.rust_bridge");
        if check.is_err() {
            println!("Skipping test: vLLM not available");
            return;
        }

        let mut node = InferenceNode::new("gpt2".to_string(), Some(1), Some(0.3));

        if node.initialize(Some(1), Some(0.3)).is_err() {
            println!("Skipping test: Failed to initialize engine");
            return;
        }

        // First request
        let req1 = InferenceRequest {
            request_id: "req-1".to_string(),
            prompt: "Hello".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 0.9,
            stream: false,
        };

        let resp1 = node.inference(&req1);
        assert!(resp1.is_ok());

        // Second request
        let req2 = InferenceRequest {
            request_id: "req-2".to_string(),
            prompt: "Goodbye".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 0.9,
            stream: false,
        };

        let resp2 = node.inference(&req2);
        assert!(resp2.is_ok());

        let r1 = resp1.unwrap();
        let r2 = resp2.unwrap();
        assert_ne!(r1.generated_text, r2.generated_text);

        let _ = node.shutdown();
    });
}
