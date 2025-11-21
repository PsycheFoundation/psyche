/// Integration tests for vLLM Rust-Python FFI
///
/// These tests verify that the Rust bindings correctly interface with
/// the Python vLLM implementation.
///
/// Prerequisites:
/// - vLLM must be installed: pip install vllm
/// - PyTorch must be installed
/// - psyche Python package must be in PYTHONPATH
use psyche_inference::vllm::{
    SamplingParams, VLLMEngine, VLLMMode, VLLMWithUpdater, VLLMWithUpdaterConfig,
};
use std::collections::HashMap;
use tch::Tensor;

/// Test basic engine initialization and inference
#[test]
#[ignore] // Requires vLLM installed
fn test_engine_basic_inference() {
    let config = psyche_inference::vllm::VLLMConfig::new("gpt2")
        .with_max_len(512)
        .with_gpu_memory(0.3);

    let engine = VLLMEngine::new(config).expect("Failed to create engine");

    let params = SamplingParams::greedy().with_max_tokens(10);

    let request_id = engine
        .add_request("Hello, world!", params)
        .expect("Failed to add request");

    assert!(!request_id.is_empty());

    // Process at least one step
    let outputs = engine.step().expect("Failed to step");
    assert!(outputs.len() <= 1); // Should have 0 or 1 output

    // Check if there are unfinished requests
    let has_unfinished = engine
        .has_unfinished_requests()
        .expect("Failed to check unfinished requests");

    println!("Has unfinished requests: {}", has_unfinished);
}

/// Test VLLMWithUpdater in training mode
#[test]
#[ignore] // Requires vLLM installed
fn test_updater_training_mode() {
    let vllm = VLLMWithUpdater::for_training("gpt2").expect("Failed to create VLLMWithUpdater");

    // Verify we can get the engine in direct mode
    let engine = vllm.get_engine().expect("Failed to get engine");

    // Run a simple inference
    let params = SamplingParams::default().with_max_tokens(5);
    let request_id = engine
        .add_request("Test", params)
        .expect("Failed to add request");

    assert!(!request_id.is_empty());

    // Clean shutdown
    vllm.shutdown().expect("Failed to shutdown");
}

/// Test VLLMWithUpdater configuration builders
#[test]
fn test_config_builders() {
    let config = VLLMWithUpdaterConfig::for_training("gpt2")
        .with_tensor_parallel(2)
        .with_gpu_memory(0.7)
        .with_max_len(1024);

    assert_eq!(config.model_name, "gpt2");
    assert_eq!(config.tensor_parallel_size, 2);
    assert_eq!(config.gpu_memory_utilization, 0.7);
    assert_eq!(config.max_model_len, Some(1024));
    assert_eq!(config.mode, VLLMMode::Direct);

    let server_config = VLLMWithUpdaterConfig::for_atropos("gpt2", 8000);
    assert_eq!(server_config.mode, VLLMMode::Server);
    assert_eq!(server_config.server_port, Some(8000));
}

/// Test weight update (mock data)
#[test]
#[ignore] // Requires vLLM installed
fn test_weight_update() {
    let vllm = VLLMWithUpdater::for_training("gpt2").expect("Failed to create VLLMWithUpdater");

    // Create a small mock weight delta
    let mut delta = HashMap::new();
    let small_tensor = Tensor::randn(&[768, 768], tch::kind::FLOAT_CPU) * 0.001;
    delta.insert(
        "transformer.h.0.attn.c_attn.weight".to_string(),
        small_tensor,
    );

    // This should succeed (updater will process it asynchronously)
    vllm.update_weights(delta)
        .expect("Failed to update weights");

    // Create a checkpoint
    vllm.checkpoint().expect("Failed to checkpoint");

    vllm.shutdown().expect("Failed to shutdown");
}

/// Test that server mode correctly rejects direct engine access
#[test]
#[ignore] // Requires vLLM installed
fn test_server_mode_rejects_engine_access() {
    let vllm =
        VLLMWithUpdater::for_atropos("gpt2", 9000).expect("Failed to create VLLMWithUpdater");

    // Should fail - can't get engine in server mode
    let result = vllm.get_engine();
    assert!(result.is_err());

    // Should get server URL
    let url = vllm.server_url();
    assert!(url.is_some());
    assert_eq!(url.unwrap(), "http://localhost:9000");

    vllm.shutdown().expect("Failed to shutdown");
}

/// Test sampling params builder
#[test]
fn test_sampling_params() {
    let params = SamplingParams::default()
        .with_temperature(0.8)
        .with_top_p(0.95)
        .with_max_tokens(100)
        .with_seed(42);

    assert_eq!(params.temperature, 0.8);
    assert_eq!(params.top_p, 0.95);
    assert_eq!(params.max_tokens, 100);
    assert_eq!(params.seed, Some(42));

    let greedy = SamplingParams::greedy();
    assert_eq!(greedy.temperature, 0.0);
}
