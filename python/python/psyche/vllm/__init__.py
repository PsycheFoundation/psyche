"""
vLLM Integration for Psyche

This module provides vLLM inference with live weight updates from Psyche training
via torch.distributed.

Key Components:
- UpdatableLLMEngine: vLLM wrapper that uses patched GPUModelRunner
- distributed_updater: Process that receives weight updates via torch.distributed
- vllm_patch: Patches GPUModelRunner to spawn distributed updater
- transforms: Weight transformations (QKV fusion, rotary permutation, etc.)
"""

from .engine import UpdatableLLMEngine, VLLM_AVAILABLE
from .transforms import (
    apply_qkv_fusion,
    apply_gate_up_fusion,
    apply_rotary_permute,
    permute_for_rotary,
    build_full_transform_config_llama,
)

__all__ = [
    # Engine
    "UpdatableLLMEngine",
    "VLLM_AVAILABLE",
    # Transforms
    "apply_qkv_fusion",
    "apply_gate_up_fusion",
    "apply_rotary_permute",
    "permute_for_rotary",
    "build_full_transform_config_llama",
]

__version__ = "0.1.0"
