// Test subprocess-based checkpoint reload pattern from Rust
#[cfg(feature = "vllm-tests")]
use std::process::{Command, Stdio};

#[test]
#[cfg(feature = "vllm-tests")]
fn test_subprocess_checkpoint_reload() {
    if !check_vllm_available() {
        println!("Skipping test: vLLM not available");
        return;
    }

    println!("\n=== Subprocess Checkpoint Reload Test (Rust) ===");

    let prompt = "Once upon a time";

    println!("\n[1] Spawning subprocess 1 with GPT2...");
    let output1 = spawn_and_run_inference("gpt2", prompt).expect("Subprocess 1 failed");
    println!("   Output1: {:?}", output1);

    println!("\n[2] Spawning subprocess 2 with GPT2...");
    let output2 = spawn_and_run_inference("gpt2", prompt).expect("Subprocess 2 failed");
    println!("   Output2: {:?}", output2);

    assert_eq!(
        output1, output2,
        "Outputs should be identical for same model+prompt"
    );

    println!("\n=== Test Passed: Subprocess reload pattern works! ===\n");
}

#[cfg(feature = "vllm-tests")]
fn check_vllm_available() -> bool {
    Command::new("python3")
        .args(["-c", "import vllm; import torch"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(feature = "vllm-tests")]
fn spawn_and_run_inference(model_path: &str, prompt: &str) -> std::io::Result<String> {
    let script_path = std::env::current_dir()?
        .join("../../python/python/psyche/vllm/run_inference_subprocess.py");

    let output = Command::new("python3")
        .arg(&script_path)
        .arg(model_path)
        .arg(prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit()) // Show errors
        .output()?;

    if !output.status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Subprocess failed",
        ));
    }

    // Parse JSON output
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().rev() {
        if line.starts_with('{') {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(text) = json.get("generated_text").and_then(|v| v.as_str()) {
                    return Ok(text.to_string());
                }
            }
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "Failed to parse JSON output",
    ))
}
