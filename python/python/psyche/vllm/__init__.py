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
from .vllm_patch import get_shared_state_dict_from_engine
from .transforms import (
    apply_qkv_fusion,
    apply_gate_up_fusion,
    apply_rotary_permute,
    permute_for_rotary,
    build_full_transform_config_llama,
)
from .protocol import (
    broadcast_parameter,
    broadcast_state_dict,
    broadcast_shutdown_signal,
)

__all__ = [
    # Engine
    "UpdatableLLMEngine",
    "VLLM_AVAILABLE",
    # Testing/Debug utilities
    "get_shared_state_dict_from_engine",
    # Transforms
    "apply_qkv_fusion",
    "apply_gate_up_fusion",
    "apply_rotary_permute",
    "permute_for_rotary",
    "build_full_transform_config_llama",
    # Protocol
    "broadcast_parameter",
    "broadcast_state_dict",
    "broadcast_shutdown_signal",
]

__version__ = "0.1.0"
