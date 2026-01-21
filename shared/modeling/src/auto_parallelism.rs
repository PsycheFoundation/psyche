//! Auto-parallelism calculation for optimal DP, TP, and micro_batch_size configuration.
//!
//! This module automatically determines the best parallelism settings based on:
//! - Model size (estimated from config.json parameters)
//! - Hardware capabilities (currently H100 only)
//! - Run configuration (optimizer, batch sizes, etc.)

use psyche_core::OptimizerDefinition;
use std::process::Command;
use tracing::{info, warn};

use crate::{DeepseekConfig, LlamaConfig};

/// H100 VRAM in GB
const H100_VRAM_GB: f64 = 80.0;

/// Fallback VRAM for unknown GPUs (very conservative)
const FALLBACK_VRAM_GB: f64 = 16.0;

/// Safety margin multiplier to avoid OOM (30%)
const MEMORY_SAFETY_MARGIN: f64 = 1.3;

/// Detected hardware profile
#[derive(Debug, Clone)]
pub struct HardwareProfile {
    pub gpu_name: String,
    pub vram_gb: f64,
    pub num_gpus: usize,
}

impl HardwareProfile {
    /// Conservative fallback for unknown or unavailable hardware
    pub fn fallback() -> Self {
        Self {
            gpu_name: "unknown".to_string(),
            vram_gb: FALLBACK_VRAM_GB,
            num_gpus: 1,
        }
    }

    /// Check if this is a recognized/supported hardware profile
    pub fn is_supported(&self) -> bool {
        self.gpu_name.to_lowercase().contains("h100")
    }
}

/// Estimated memory requirements for a model
#[derive(Debug, Clone)]
pub struct ModelMemoryEstimate {
    pub total_params: u64,
    pub model_memory_gb: f64,
    pub optimizer_memory_gb: f64,
    pub total_memory_gb: f64,
}

/// Computed parallelism configuration
#[derive(Debug, Clone, Copy)]
pub struct ComputedParallelism {
    pub data_parallelism: usize,
    pub tensor_parallelism: usize,
    pub micro_batch_size: usize,
}

impl Default for ComputedParallelism {
    fn default() -> Self {
        Self {
            data_parallelism: 1,
            tensor_parallelism: 1,
            micro_batch_size: 1,
        }
    }
}

/// Trait for model configs that can estimate parameter count
pub trait ModelParamEstimator {
    fn estimate_params(&self) -> u64;
}

impl ModelParamEstimator for LlamaConfig {
    fn estimate_params(&self) -> u64 {
        estimate_llama_params(self)
    }
}

impl ModelParamEstimator for DeepseekConfig {
    fn estimate_params(&self) -> u64 {
        estimate_deepseek_params(self)
    }
}

/// Estimate parameters for Llama-style models
fn estimate_llama_params(config: &LlamaConfig) -> u64 {
    let h = config.hidden_size as u64;
    let v = config.vocab_size as u64;
    let l = config.num_hidden_layers as u64;
    let i = config.intermediate_size as u64;
    let kv_heads = config
        .num_key_value_heads
        .unwrap_or(config.num_attention_heads) as u64;
    let q_heads = config.num_attention_heads as u64;

    // Embeddings (input + output, assume not tied for safety)
    let embed_params = if config.tie_word_embeddings {
        v * h
    } else {
        v * h * 2
    };

    // Per-layer params
    let head_dim = h / q_heads;

    // Attention projections (with GQA support)
    let q_params = h * h; // Q projection
    let k_params = h * (kv_heads * head_dim); // K projection (GQA)
    let v_params = h * (kv_heads * head_dim); // V projection (GQA)
    let o_params = h * h; // O projection
    let attn_params = q_params + k_params + v_params + o_params;

    // MLP (gate, up, down projections for SwiGLU)
    let mlp_params = h * i * 3;

    // Layer norms (2 per layer: attention + MLP)
    let norm_params = h * 2;

    let layer_params = attn_params + mlp_params + norm_params;

    // Final layer norm
    let final_norm_params = h;

    embed_params + (l * layer_params) + final_norm_params
}

/// Estimate parameters for DeepSeek-style models (includes MoE)
fn estimate_deepseek_params(config: &DeepseekConfig) -> u64 {
    let h = config.hidden_size as u64;
    let v = config.vocab_size as u64;
    let l = config.num_hidden_layers as u64;
    let i = config.intermediate_size as u64;

    // Embeddings
    let embed_params = if config.tie_word_embeddings {
        v * h
    } else {
        v * h * 2
    };

    // For MoE models, estimate is more complex
    let moe_intermediate = config
        .moe_intermediate_size
        .unwrap_or(config.intermediate_size) as u64;
    let n_experts = config.n_routed_experts.unwrap_or(1) as u64;
    let n_shared = config.n_shared_experts.unwrap_or(0) as u64;
    let moe_freq = config.moe_layer_freq.unwrap_or(1) as u64;
    let first_k_dense = config.first_k_dense_replace.unwrap_or(0) as u64;

    // Attention params per layer (simplified - MLA has different structure)
    let attn_params = h * h * 4; // Conservative estimate

    // Dense MLP params
    let dense_mlp_params = h * i * 3;

    // MoE MLP params (per expert)
    let expert_params = h * moe_intermediate * 3;
    let router_params = h * n_experts;

    // Count MoE vs dense layers
    let num_moe_layers = if moe_freq > 0 && n_experts > 1 {
        ((l - first_k_dense) / moe_freq).max(0)
    } else {
        0
    };
    let num_dense_layers = l - num_moe_layers;

    // Layer params
    let dense_layer_params = attn_params + dense_mlp_params + h * 2;
    let moe_layer_params = attn_params
        + (expert_params * n_experts)
        + (expert_params * n_shared)
        + router_params
        + h * 2;

    let total_layer_params =
        (num_dense_layers * dense_layer_params) + (num_moe_layers * moe_layer_params);

    embed_params + total_layer_params + h
}

/// Estimate memory requirements for training
pub fn estimate_memory(model_params: u64, optimizer: &OptimizerDefinition) -> ModelMemoryEstimate {
    let precision_bytes: u64 = 2; // bf16
    let model_memory_bytes = model_params * precision_bytes;

    // Conservative optimizer memory multipliers
    let optimizer_multiplier = match optimizer {
        OptimizerDefinition::AdamW { .. } => 2.0,  // m + v states
        OptimizerDefinition::Distro { .. } => 2.0, // Conservative (same as AdamW)
        OptimizerDefinition::Dummy => 0.0,
    };

    let optimizer_memory_bytes = (model_memory_bytes as f64 * optimizer_multiplier) as u64;
    let total_bytes = model_memory_bytes + optimizer_memory_bytes;

    ModelMemoryEstimate {
        total_params: model_params,
        model_memory_gb: (model_memory_bytes as f64 / 1e9) * MEMORY_SAFETY_MARGIN,
        optimizer_memory_gb: (optimizer_memory_bytes as f64 / 1e9) * MEMORY_SAFETY_MARGIN,
        total_memory_gb: (total_bytes as f64 / 1e9) * MEMORY_SAFETY_MARGIN,
    }
}

/// Get GPU name using nvidia-smi
fn get_gpu_name_from_nvidia_smi() -> Option<String> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Get first GPU name (assume homogeneous setup)
    stdout.lines().next().map(|s| s.trim().to_string())
}

/// Detect hardware profile
pub fn detect_hardware() -> HardwareProfile {
    let num_gpus = tch::Cuda::device_count() as usize;

    if num_gpus == 0 {
        warn!("No CUDA devices detected, using fallback configuration");
        return HardwareProfile::fallback();
    }

    // Try to get GPU name
    let gpu_name = get_gpu_name_from_nvidia_smi().unwrap_or_else(|| "unknown".to_string());
    let name_lower = gpu_name.to_lowercase();

    // Only recognize H100
    if name_lower.contains("h100") {
        info!("Detected {} x H100 GPU(s)", num_gpus);
        HardwareProfile {
            gpu_name,
            vram_gb: H100_VRAM_GB,
            num_gpus,
        }
    } else {
        warn!(
            "Unsupported GPU '{}', using fallback configuration",
            gpu_name
        );
        HardwareProfile {
            gpu_name,
            vram_gb: FALLBACK_VRAM_GB,
            num_gpus: 1, // Conservative: only use 1 GPU for unknown hardware
        }
    }
}

/// Calculate optimal parallelism configuration
pub fn calculate_parallelism(
    hardware: &HardwareProfile,
    memory: &ModelMemoryEstimate,
    max_seq_len: u32,
    global_batch_size: u16,
    min_clients: u16,
) -> ComputedParallelism {
    let vram_per_gpu = hardware.vram_gb;
    let num_gpus = hardware.num_gpus;

    // Step 1: Determine TP (model must fit in GPU memory)
    let tensor_parallelism = if memory.total_memory_gb <= vram_per_gpu {
        1
    } else {
        let min_tp = (memory.total_memory_gb / vram_per_gpu).ceil() as usize;
        // Round up to power of 2, but don't exceed num_gpus
        min_tp.next_power_of_two().min(num_gpus)
    };

    // Step 2: Determine DP (use remaining GPUs)
    let data_parallelism = (num_gpus / tensor_parallelism).max(1);

    // Step 3: Calculate micro_batch_size based on remaining VRAM
    let model_vram_per_gpu = memory.total_memory_gb / tensor_parallelism as f64;
    let available_for_activations = (vram_per_gpu - model_vram_per_gpu).max(0.0);

    // Conservative activation memory estimate per sample
    // Roughly: num_layers * hidden_size * seq_len * 4 bytes * factor
    // Simplified to: model_params * empirical_factor * seq_len_ratio
    let activation_per_sample_gb =
        (memory.total_params as f64 / 1e9) * 0.015 * (max_seq_len as f64 / 2048.0);

    let max_micro_batch = if activation_per_sample_gb > 0.0 {
        (available_for_activations / activation_per_sample_gb).floor() as usize
    } else {
        1
    };

    // Clamp micro_batch_size to reasonable range
    let mut micro_batch_size = max_micro_batch.max(1).min(16);

    // Validate: micro_batch_size should ideally divide max batches per client
    let max_batches_per_client = if min_clients > 0 {
        (global_batch_size as usize).div_ceil(min_clients as usize)
    } else {
        global_batch_size as usize
    };

    if max_batches_per_client > 0 && micro_batch_size > 1 {
        // Find largest divisor <= micro_batch_size
        micro_batch_size = (1..=micro_batch_size)
            .rev()
            .find(|&mbs| max_batches_per_client % mbs == 0)
            .unwrap_or(1);
    }

    ComputedParallelism {
        data_parallelism,
        tensor_parallelism,
        micro_batch_size,
    }
}

/// Main entry point: compute parallelism from model config and run parameters
pub fn compute_auto_parallelism<C: ModelParamEstimator>(
    model_config: &C,
    optimizer: &OptimizerDefinition,
    max_seq_len: u32,
    global_batch_size: u16,
    min_clients: u16,
) -> ComputedParallelism {
    // Detect hardware
    let hardware = detect_hardware();

    // Estimate model parameters and memory
    let model_params = model_config.estimate_params();
    let memory = estimate_memory(model_params, optimizer);

    // Calculate parallelism
    let result = calculate_parallelism(
        &hardware,
        &memory,
        max_seq_len,
        global_batch_size,
        min_clients,
    );

    // Log the results
    info!("=== Auto-parallelism Configuration ===");
    info!(
        "Hardware: {} x {} ({:.1} GB VRAM each)",
        hardware.num_gpus, hardware.gpu_name, hardware.vram_gb
    );
    info!("Model size: {:.2}B parameters", model_params as f64 / 1e9);
    info!(
        "Memory estimate: {:.1} GB (model) + {:.1} GB (optimizer) = {:.1} GB total",
        memory.model_memory_gb, memory.optimizer_memory_gb, memory.total_memory_gb
    );
    info!(
        "Calculated: DP={}, TP={}, micro_batch_size={}",
        result.data_parallelism, result.tensor_parallelism, result.micro_batch_size
    );
    info!("=======================================");

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn llama_8b_config() -> LlamaConfig {
        // Approximate Llama 3.1 8B config
        LlamaConfig {
            hidden_size: 4096,
            intermediate_size: 14336,
            vocab_size: 128256,
            num_hidden_layers: 32,
            num_attention_heads: 32,
            num_key_value_heads: Some(8),
            rms_norm_eps: 1e-5,
            rope_theta: 500000.0,
            bos_token_id: Some(128000),
            eos_token_id: None,
            rope_scaling: None,
            max_position_embeddings: 131072,
            tie_word_embeddings: false,
            attention_bias: Some(false),
        }
    }

    fn llama_70b_config() -> LlamaConfig {
        // Approximate Llama 70B config
        LlamaConfig {
            hidden_size: 8192,
            intermediate_size: 28672,
            vocab_size: 128256,
            num_hidden_layers: 80,
            num_attention_heads: 64,
            num_key_value_heads: Some(8),
            rms_norm_eps: 1e-5,
            rope_theta: 500000.0,
            bos_token_id: Some(128000),
            eos_token_id: None,
            rope_scaling: None,
            max_position_embeddings: 131072,
            tie_word_embeddings: false,
            attention_bias: Some(false),
        }
    }

    fn llama_405b_config() -> LlamaConfig {
        // Approximate Llama 405B config
        LlamaConfig {
            hidden_size: 16384,
            intermediate_size: 53248,
            vocab_size: 128256,
            num_hidden_layers: 126,
            num_attention_heads: 128,
            num_key_value_heads: Some(8),
            rms_norm_eps: 1e-5,
            rope_theta: 500000.0,
            bos_token_id: Some(128000),
            eos_token_id: None,
            rope_scaling: None,
            max_position_embeddings: 131072,
            tie_word_embeddings: false,
            attention_bias: Some(false),
        }
    }

    #[test]
    fn test_llama_8b_param_estimation() {
        let config = llama_8b_config();
        let params = estimate_llama_params(&config);
        let params_billions = params as f64 / 1e9;

        // Should be approximately 8B (allow 20% tolerance)
        assert!(
            params_billions > 6.0 && params_billions < 10.0,
            "Expected ~8B params, got {:.2}B",
            params_billions
        );
    }

    #[test]
    fn test_llama_70b_param_estimation() {
        let config = llama_70b_config();
        let params = estimate_llama_params(&config);
        let params_billions = params as f64 / 1e9;

        // Should be approximately 70B (allow 20% tolerance)
        assert!(
            params_billions > 56.0 && params_billions < 84.0,
            "Expected ~70B params, got {:.2}B",
            params_billions
        );
    }

    #[test]
    fn test_llama_405b_param_estimation() {
        let config = llama_405b_config();
        let params = estimate_llama_params(&config);
        let params_billions = params as f64 / 1e9;

        // Should be approximately 405B (allow 20% tolerance)
        assert!(
            params_billions > 324.0 && params_billions < 486.0,
            "Expected ~405B params, got {:.2}B",
            params_billions
        );
    }

    #[test]
    fn test_memory_estimation_adamw() {
        let params = 8_000_000_000u64; // 8B
        let optimizer = OptimizerDefinition::AdamW {
            betas: [0.9, 0.999],
            weight_decay: 0.1,
            eps: 1e-8,
            clip_grad_norm: Some(1.0),
        };

        let memory = estimate_memory(params, &optimizer);

        // 8B params * 2 bytes = 16 GB model
        // 16 GB * 2 (optimizer) = 32 GB optimizer
        // Total = 48 GB * 1.3 (safety) = 62.4 GB
        assert!(
            memory.total_memory_gb > 60.0 && memory.total_memory_gb < 70.0,
            "Expected ~62GB, got {:.1}GB",
            memory.total_memory_gb
        );
    }

    #[test]
    fn test_parallelism_8b_on_8_h100() {
        let hardware = HardwareProfile {
            gpu_name: "H100".to_string(),
            vram_gb: 80.0,
            num_gpus: 8,
        };

        let memory = ModelMemoryEstimate {
            total_params: 8_000_000_000,
            model_memory_gb: 20.8,     // 16 * 1.3
            optimizer_memory_gb: 41.6, // 32 * 1.3
            total_memory_gb: 62.4,
        };

        let result = calculate_parallelism(&hardware, &memory, 4096, 256, 8);

        // 62.4 GB fits in 80 GB, so TP=1, DP=8
        assert_eq!(result.tensor_parallelism, 1);
        assert_eq!(result.data_parallelism, 8);
    }

    #[test]
    fn test_parallelism_70b_on_8_h100() {
        let hardware = HardwareProfile {
            gpu_name: "H100".to_string(),
            vram_gb: 80.0,
            num_gpus: 8,
        };

        let memory = ModelMemoryEstimate {
            total_params: 70_000_000_000,
            model_memory_gb: 182.0,     // 140 * 1.3
            optimizer_memory_gb: 364.0, // 280 * 1.3
            total_memory_gb: 546.0,
        };

        let result = calculate_parallelism(&hardware, &memory, 4096, 256, 8);

        // 546 GB needs at least 7 GPUs, rounds to 8
        assert_eq!(result.tensor_parallelism, 8);
        assert_eq!(result.data_parallelism, 1);
    }

    #[test]
    fn test_parallelism_70b_on_16_h100() {
        let hardware = HardwareProfile {
            gpu_name: "H100".to_string(),
            vram_gb: 80.0,
            num_gpus: 16,
        };

        let memory = ModelMemoryEstimate {
            total_params: 70_000_000_000,
            model_memory_gb: 182.0,
            optimizer_memory_gb: 364.0,
            total_memory_gb: 546.0,
        };

        let result = calculate_parallelism(&hardware, &memory, 4096, 256, 8);

        // 546 GB needs 8 GPUs for TP, leaving 8 for DP
        assert_eq!(result.tensor_parallelism, 8);
        assert_eq!(result.data_parallelism, 2);
    }

    #[test]
    fn test_fallback_for_unknown_gpu() {
        let hardware = HardwareProfile {
            gpu_name: "unknown".to_string(),
            vram_gb: 16.0,
            num_gpus: 1,
        };

        let memory = ModelMemoryEstimate {
            total_params: 8_000_000_000,
            model_memory_gb: 20.8,
            optimizer_memory_gb: 41.6,
            total_memory_gb: 62.4,
        };

        let result = calculate_parallelism(&hardware, &memory, 4096, 256, 8);

        // Can't fit, but we do our best with 1 GPU
        assert_eq!(result.data_parallelism, 1);
    }
}
