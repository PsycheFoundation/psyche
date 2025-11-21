/// Example demonstrating vLLM with live weight updates
///
/// This example shows how to:
/// 1. Initialize vLLM in training mode
/// 2. Run inference
/// 3. Update weights from training
/// 4. Create checkpoints for error recovery
///
/// Run with: cargo run --example vllm_with_updates
use psyche_inference::vllm::{
    SamplingParams, VLLMEngine, VLLMWithUpdater, VLLMWithUpdaterConfig, WeightDeltaBatch,
};
use std::collections::HashMap;
use tch::Tensor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== vLLM with Live Weight Updates Example ===\n");

    // Example 1: Training Mode (Direct API)
    example_training_mode()?;

    // Example 2: Server Mode (OpenAI API for Atropos)
    example_server_mode()?;

    Ok(())
}

fn example_training_mode() -> Result<(), Box<dyn std::error::Error>> {
    println!("Example 1: Training Mode (Direct API)");
    println!("---------------------------------------");

    // Initialize vLLM for training
    println!("Initializing vLLM for training with gpt2...");
    let vllm = VLLMWithUpdater::for_training("gpt2")?;

    // Get the engine for inference
    println!("Getting engine for inference...");
    let engine = vllm.get_engine()?;

    // Run inference
    println!("Running initial inference...");
    let params = SamplingParams::default()
        .with_temperature(0.8)
        .with_max_tokens(50);

    let request_id = engine.add_request("The capital of France is", params)?;
    println!("Added request: {}", request_id);

    // Process until complete
    println!("Processing inference...");
    loop {
        let outputs = engine.step()?;

        for output in outputs {
            if output.finished {
                println!("Generated text: {}", output.text);
                break;
            }
        }

        if !engine.has_unfinished_requests()? {
            break;
        }
    }

    // Simulate weight update from training
    println!("\nSimulating weight update from training...");
    let mut weight_delta = create_mock_weight_delta();

    println!("Applying weight update...");
    vllm.update_weights(weight_delta)?;
    println!("Weight update applied successfully!");

    // Create checkpoint for error recovery
    println!("\nCreating checkpoint...");
    vllm.checkpoint()?;
    println!("Checkpoint created successfully!");

    // Run inference again with updated weights
    println!("\nRunning inference with updated weights...");
    let params = SamplingParams::greedy().with_max_tokens(30);
    let request_id = engine.add_request("Once upon a time", params)?;

    loop {
        let outputs = engine.step()?;

        for output in outputs {
            if output.finished {
                println!("Generated text: {}", output.text);
                break;
            }
        }

        if !engine.has_unfinished_requests()? {
            break;
        }
    }

    // Shutdown
    println!("\nShutting down vLLM...");
    vllm.shutdown()?;
    println!("Training mode example complete!\n");

    Ok(())
}

fn example_server_mode() -> Result<(), Box<dyn std::error::Error>> {
    println!("Example 2: Server Mode (OpenAI API for Atropos)");
    println!("------------------------------------------------");

    // Initialize vLLM in server mode for Atropos
    println!("Initializing vLLM in server mode on port 8000...");
    let vllm = VLLMWithUpdater::for_atropos("gpt2", 8000)?;

    // Get server URL
    if let Some(url) = vllm.server_url() {
        println!("vLLM server running at: {}", url);
        println!("Atropos can now connect to this endpoint!");
    }

    // Note: In server mode, we can't use the direct engine API
    // Instead, Atropos will make OpenAI-compatible requests to the server

    // However, we can still update weights
    println!("\nSimulating weight update in server mode...");
    let weight_delta = create_mock_weight_delta();

    // Note: This will return an error in current implementation
    // as server mode weight updates are not yet supported
    match vllm.update_weights(weight_delta) {
        Ok(_) => println!("Weight update applied!"),
        Err(e) => println!("Weight update not supported in server mode: {}", e),
    }

    // Let server run for a bit (in real use, this would run indefinitely)
    println!("\nServer is running... (press Ctrl+C to stop)");
    println!("In production, Atropos would be making requests to this server.");

    // Shutdown
    println!("\nShutting down vLLM server...");
    vllm.shutdown()?;
    println!("Server mode example complete!\n");

    Ok(())
}

fn create_mock_weight_delta() -> WeightDeltaBatch {
    // Create a small mock weight delta
    // In real training, this would come from compute_weight_delta()
    let mut delta = HashMap::new();

    // Example: small random delta for a single layer
    // In practice, you'd have deltas for many parameters
    let small_delta = Tensor::randn(&[768, 768], tch::kind::FLOAT_CPU) * 0.001;

    delta.insert(
        "transformer.h.0.attn.c_attn.weight".to_string(),
        small_delta,
    );

    delta
}

// Advanced example: Using compute_weight_delta helper
#[allow(dead_code)]
fn example_compute_weight_delta() -> Result<(), Box<dyn std::error::Error>> {
    use psyche_inference::vllm::compute_weight_delta;
    use pyo3::prelude::*;

    println!("Example 3: Computing Weight Delta");
    println!("-----------------------------------");

    Python::with_gil(|py| -> Result<(), Box<dyn std::error::Error>> {
        // Load two model checkpoints (current and reference)
        // This would typically be your training model and reference model
        let transformers = PyModule::import(py, "transformers")?;

        println!("Loading models...");
        let current_model = transformers
            .getattr("AutoModelForCausalLM")?
            .call_method1("from_pretrained", ("gpt2",))?;

        let reference_model = transformers
            .getattr("AutoModelForCausalLM")?
            .call_method1("from_pretrained", ("gpt2",))?;

        // Compute delta: Δw = w_current - w_reference
        println!("Computing weight delta...");
        let delta = compute_weight_delta(&current_model.into(), &reference_model.into())?;

        println!("Computed delta for {} parameters", delta.len());

        Ok(())
    })?;

    Ok(())
}

// Example showing checkpoint and restore
#[allow(dead_code)]
fn example_checkpoint_restore() -> Result<(), Box<dyn std::error::Error>> {
    println!("Example 4: Checkpoint and Restore");
    println!("----------------------------------");

    let vllm = VLLMWithUpdater::for_training("gpt2")?;

    // Create initial checkpoint
    println!("Creating checkpoint 1...");
    vllm.checkpoint()?;

    // Apply some weight updates
    println!("Applying weight updates...");
    vllm.update_weights(create_mock_weight_delta())?;

    // Create another checkpoint
    println!("Creating checkpoint 2...");
    vllm.checkpoint()?;

    // Apply more updates
    println!("Applying more weight updates...");
    vllm.update_weights(create_mock_weight_delta())?;

    // If something goes wrong, the updater daemon will automatically
    // restore from the last checkpoint

    println!("Checkpointing example complete!");
    vllm.shutdown()?;

    Ok(())
}
