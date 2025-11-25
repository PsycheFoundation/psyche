"""
Unit tests for weight transformations.

These tests verify the transformation logic without requiring vLLM.
"""

import torch
import pytest
from psyche.vllm.transforms import (
    permute_for_rotary,
    apply_qkv_fusion,
    apply_gate_up_fusion,
    build_full_transform_config_llama,
)


def test_permute_for_rotary():
    """Test rotary permutation"""
    n_heads = 4
    head_dim = 8
    in_dim = 128

    # Create test weight
    weight = torch.randn(n_heads * head_dim, in_dim)

    # Apply permutation
    permuted = permute_for_rotary(weight, n_heads)

    # Check shape unchanged
    assert permuted.shape == weight.shape

    # Apply permutation twice should return to original
    # (rotary permutation is its own inverse)
    double_permuted = permute_for_rotary(permuted, n_heads)
    torch.testing.assert_close(double_permuted, weight)


def test_qkv_fusion():
    """Test QKV fusion"""
    n_heads = 4
    head_dim = 8
    in_dim = 128

    # Create Q, K, V weights
    q_weight = torch.randn(n_heads * head_dim, in_dim)
    k_weight = torch.randn(n_heads * head_dim, in_dim)
    v_weight = torch.randn(n_heads * head_dim, in_dim)

    # Fuse without rotary
    fused = apply_qkv_fusion(q_weight, k_weight, v_weight, n_heads, apply_rotary=False)

    # Check output shape
    expected_out_dim = 3 * n_heads * head_dim
    assert fused.shape == (expected_out_dim, in_dim)

    # Check concatenation order (Q, K, V)
    q_size = n_heads * head_dim
    torch.testing.assert_close(fused[:q_size], q_weight)
    torch.testing.assert_close(fused[q_size : 2 * q_size], k_weight)
    torch.testing.assert_close(fused[2 * q_size :], v_weight)


def test_qkv_fusion_with_rotary():
    """Test QKV fusion with rotary permutation"""
    n_heads = 4
    head_dim = 8
    in_dim = 128

    # Create Q, K, V weights
    q_weight = torch.randn(n_heads * head_dim, in_dim)
    k_weight = torch.randn(n_heads * head_dim, in_dim)
    v_weight = torch.randn(n_heads * head_dim, in_dim)

    # Fuse with rotary
    fused = apply_qkv_fusion(q_weight, k_weight, v_weight, n_heads, apply_rotary=True)

    # Check output shape
    expected_out_dim = 3 * n_heads * head_dim
    assert fused.shape == (expected_out_dim, in_dim)

    # Q and K should be permuted, V should not
    q_size = n_heads * head_dim
    q_permuted = permute_for_rotary(q_weight, n_heads)
    k_permuted = permute_for_rotary(k_weight, n_heads)

    torch.testing.assert_close(fused[:q_size], q_permuted)
    torch.testing.assert_close(fused[q_size : 2 * q_size], k_permuted)
    torch.testing.assert_close(fused[2 * q_size :], v_weight)


def test_qkv_fusion_gqa():
    """Test QKV fusion with Grouped Query Attention"""
    n_heads = 8
    n_kv_heads = 2  # GQA: fewer K/V heads
    head_dim = 8
    in_dim = 128

    # Create Q, K, V weights with different output dimensions
    q_weight = torch.randn(n_heads * head_dim, in_dim)
    k_weight = torch.randn(n_kv_heads * head_dim, in_dim)
    v_weight = torch.randn(n_kv_heads * head_dim, in_dim)

    # Fuse
    fused = apply_qkv_fusion(
        q_weight, k_weight, v_weight, n_heads, n_kv_heads, apply_rotary=False
    )

    # Check output shape
    expected_out_dim = (n_heads + 2 * n_kv_heads) * head_dim
    assert fused.shape == (expected_out_dim, in_dim)


def test_gate_up_fusion():
    """Test gate-up fusion"""
    intermediate_dim = 256
    in_dim = 128

    # Create gate and up weights
    gate_weight = torch.randn(intermediate_dim, in_dim)
    up_weight = torch.randn(intermediate_dim, in_dim)

    # Fuse
    fused = apply_gate_up_fusion(gate_weight, up_weight)

    # Check output shape
    assert fused.shape == (2 * intermediate_dim, in_dim)

    # Check concatenation order (gate, up)
    torch.testing.assert_close(fused[:intermediate_dim], gate_weight)
    torch.testing.assert_close(fused[intermediate_dim:], up_weight)


def test_build_transform_config():
    """Test building full transform config"""
    model_config = {
        "n_layers": 2,
        "n_heads": 4,
        "n_kv_heads": 4,
        "hidden_dim": 128,
    }

    config = build_full_transform_config_llama(model_config)

    # Check we have configs for all parameters
    # Each layer has: q, k, v, gate, up = 5 parameters
    expected_params = 2 * 5
    assert len(config) == expected_params

    # Check QKV configs
    for layer_idx in range(2):
        for component in ["q", "k", "v"]:
            param_name = f"model.layers.{layer_idx}.self_attn.{component}_proj.weight"
            assert param_name in config
            assert config[param_name]["type"] == "qkv_fusion"
            assert config[param_name]["component"] == component
            assert config[param_name]["n_heads"] == 4

    # Check gate-up configs
    for layer_idx in range(2):
        for component in ["gate", "up"]:
            param_name = f"model.layers.{layer_idx}.mlp.{component}_proj.weight"
            assert param_name in config
            assert config[param_name]["type"] == "gate_up_fusion"
            assert config[param_name]["component"] == component


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
