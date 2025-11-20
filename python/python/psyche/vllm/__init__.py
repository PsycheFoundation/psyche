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

__all__ = [
    "UpdatableLLMEngine",
    "VLLM_AVAILABLE",
]

# These will be added later
# from .updater import WeightUpdater, spawn_updater_process
# from .manager import VLLMWithUpdater
# from .transforms import apply_qkv_fusion, apply_gate_up_fusion, apply_rotary_permute

__version__ = "0.1.0"
