"""
vLLM Integration for Psyche

This module provides vLLM inference with live weight updates from Psyche training.

Key Components:
- UpdatableLLMEngine: vLLM wrapper with parameter registry
- WeightUpdater: Daemon process for applying weight updates
- VLLMWithUpdater: High-level manager combining engine + updater
- Weight transformations: QKV fusion, rotary permutation, etc.
"""

from .engine import UpdatableLLMEngine, VLLM_AVAILABLE
from .updater import WeightUpdater, WeightUpdate, spawn_updater_process
from .manager import (
    VLLMWithUpdater,
    create_vllm_for_training,
    create_vllm_for_atropos,
)
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
    # Updater
    "WeightUpdater",
    "WeightUpdate",
    "spawn_updater_process",
    # Manager
    "VLLMWithUpdater",
    "create_vllm_for_training",
    "create_vllm_for_atropos",
    # Transforms
    "apply_qkv_fusion",
    "apply_gate_up_fusion",
    "apply_rotary_permute",
    "permute_for_rotary",
    "build_full_transform_config_llama",
]

__version__ = "0.1.0"
