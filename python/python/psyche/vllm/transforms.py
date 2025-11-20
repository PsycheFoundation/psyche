"""
Weight Transformation Functions for vLLM Updates

Adapted from TorchTitan's distributed_updater.py for LLaMA-style models.
These handle model-specific transformations needed when updating vLLM weights.

LLaMA Architecture Specifics:
- Q, K, V projections kept separate in training but fused in vLLM
- Gate and Up projections (w1, w3) fused in vLLM
- Rotary embeddings require weight permutation
"""

import logging
import torch
from typing import Optional, Tuple, Dict, Any

logger = logging.getLogger(__name__)


def apply_qkv_fusion(
    q_weight: torch.Tensor,
    k_weight: torch.Tensor,
    v_weight: torch.Tensor,
    n_heads: int,
    n_kv_heads: Optional[int] = None,
    apply_rotary: bool = True,
) -> torch.Tensor:
    """
    Fuse separate Q, K, V projections into single QKV projection.

    Training models often keep Q/K/V separate, but vLLM fuses them for efficiency.
    For Grouped Query Attention (GQA), K and V may have fewer heads than Q.

    Args:
        q_weight: Query projection weights [n_heads * head_dim, hidden_dim]
        k_weight: Key projection weights [n_kv_heads * head_dim, hidden_dim]
        v_weight: Value projection weights [n_kv_heads * head_dim, hidden_dim]
        n_heads: Number of query heads
        n_kv_heads: Number of key/value heads (for GQA). If None, defaults to n_heads
        apply_rotary: Whether to apply rotary permutation to Q and K

    Returns:
        Fused QKV weight tensor [output_dim, hidden_dim]
        where output_dim = q_dim + k_dim + v_dim
    """
    if n_kv_heads is None:
        n_kv_heads = n_heads

    # Apply rotary permutation to Q and K if needed
    if apply_rotary:
        q_weight = permute_for_rotary(q_weight, n_heads)
        k_weight = permute_for_rotary(k_weight, n_kv_heads)

    # Concatenate along output dimension (dim 0)
    # Order: Q, K, V
    fused = torch.cat([q_weight, k_weight, v_weight], dim=0)

    logger.debug(
        f"QKV fusion: Q{q_weight.shape} + K{k_weight.shape} + V{v_weight.shape} "
        f"-> {fused.shape}"
    )

    return fused


def apply_gate_up_fusion(
    gate_weight: torch.Tensor,
    up_weight: torch.Tensor,
) -> torch.Tensor:
    """
    Fuse gate and up projections in MLP layers.

    LLaMA-style architectures split the MLP expansion into gate (w1) and up (w3),
    but vLLM fuses them for efficient kernel execution.

    Args:
        gate_weight: Gate projection (w1) [intermediate_dim, hidden_dim]
        up_weight: Up projection (w3) [intermediate_dim, hidden_dim]

    Returns:
        Fused gate-up weight tensor [2 * intermediate_dim, hidden_dim]
    """
    # Concatenate along output dimension (dim 0)
    # Order: gate (w1), up (w3)
    fused = torch.cat([gate_weight, up_weight], dim=0)

    logger.debug(
        f"Gate-Up fusion: Gate{gate_weight.shape} + Up{up_weight.shape} "
        f"-> {fused.shape}"
    )

    return fused


def permute_for_rotary(weight: torch.Tensor, n_heads: int) -> torch.Tensor:
    """
    Permute weights for rotary positional embeddings.

    Converts from interleaved format (used in training) to grouped format
    (used in vLLM inference) for efficient RoPE application.

    Interleaved format: [x0, y0, x1, y1, x2, y2, ...]
    Grouped format:     [x0, x1, x2, ..., y0, y1, y2, ...]

    Args:
        weight: Weight tensor [out_features, in_features]
                where out_features = n_heads * head_dim
        n_heads: Number of attention heads

    Returns:
        Permuted weight tensor with same shape
    """
    out_dim, in_dim = weight.shape
    head_dim = out_dim // n_heads

    # Ensure head_dim is even (required for RoPE)
    assert head_dim % 2 == 0, f"head_dim must be even for RoPE, got {head_dim}"

    # Reshape to expose head structure
    # [n_heads, head_dim, in_features]
    reshaped = weight.view(n_heads, head_dim, in_dim)

    # Further split head_dim into pairs
    # [n_heads, head_dim//2, 2, in_features]
    split_pairs = reshaped.view(n_heads, head_dim // 2, 2, in_dim)

    # Transpose to group pairs
    # [n_heads, 2, head_dim//2, in_features]
    transposed = split_pairs.transpose(1, 2)

    # Reshape back to original shape
    # [out_features, in_features]
    permuted = transposed.reshape(out_dim, in_dim)

    logger.debug(f"Rotary permutation: {weight.shape} -> {permuted.shape}")

    return permuted


def apply_rotary_permute(
    weight: torch.Tensor,
    n_heads: int,
) -> torch.Tensor:
    """
    Standalone rotary permutation (alias for clarity).

    Args:
        weight: Weight tensor to permute
        n_heads: Number of attention heads

    Returns:
        Permuted weight tensor
    """
    return permute_for_rotary(weight, n_heads)


def infer_transform_config_llama(
    model_config: Dict[str, Any],
    layer_idx: int,
) -> Dict[str, Dict[str, Any]]:
    """
    Infer transformation configuration for a LLaMA-style model layer.

    This generates the transform config needed by the updater for a specific layer.

    Args:
        model_config: Model configuration dict with keys like:
            - n_heads: Number of attention heads
            - n_kv_heads: Number of key/value heads (for GQA)
            - hidden_dim: Hidden dimension size
        layer_idx: Index of the transformer layer

    Returns:
        Dictionary mapping parameter names to their transformation configs.

    Example:
        {
            "model.layers.0.self_attn.q_proj.weight": {
                "type": "qkv_fusion",
                "n_heads": 32,
                "n_kv_heads": 8,
                "component": "q",
            },
            "model.layers.0.mlp.gate_proj.weight": {
                "type": "gate_up_fusion",
                "component": "gate",
            },
        }
    """
    n_heads = model_config.get("n_heads", model_config.get("num_attention_heads", 32))
    n_kv_heads = model_config.get(
        "n_kv_heads", model_config.get("num_key_value_heads", n_heads)
    )

    config = {}

    # Attention projections (Q, K, V)
    # These need to be fused together, so we mark each component
    for component in ["q", "k", "v"]:
        param_name = f"model.layers.{layer_idx}.self_attn.{component}_proj.weight"
        config[param_name] = {
            "type": "qkv_fusion",
            "n_heads": n_heads,
            "n_kv_heads": n_kv_heads,
            "component": component,
            "apply_rotary": component in ["q", "k"],  # Only Q and K need rotary
        }

    # MLP projections (gate, up)
    # These need to be fused together
    for component in ["gate", "up"]:
        param_name = f"model.layers.{layer_idx}.mlp.{component}_proj.weight"
        config[param_name] = {
            "type": "gate_up_fusion",
            "component": component,
        }

    return config


def build_full_transform_config_llama(
    model_config: Dict[str, Any],
) -> Dict[str, Dict[str, Any]]:
    """
    Build complete transformation configuration for all layers of a LLaMA model.

    Args:
        model_config: Model configuration dict with keys:
            - n_layers: Number of transformer layers
            - n_heads: Number of attention heads
            - n_kv_heads: Number of key/value heads
            - hidden_dim: Hidden dimension size

    Returns:
        Complete transformation config for all parameters
    """
    n_layers = model_config.get("n_layers", model_config.get("num_hidden_layers", 32))

    full_config = {}

    for layer_idx in range(n_layers):
        layer_config = infer_transform_config_llama(model_config, layer_idx)
        full_config.update(layer_config)

    logger.info(
        f"Built transform config for {n_layers} layers "
        f"({len(full_config)} parameters)"
    )

    return full_config
