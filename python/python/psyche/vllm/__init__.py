"""
vLLM integration for Psyche.
"""

from .engine import UpdatableLLMEngine, VLLM_AVAILABLE
from .vllm_patch import get_shared_state_dict_from_engine, get_update_queue_from_engine
from .rust_bridge import init_weight_updater, update_weights_from_file, shutdown_updater

__all__ = [
    "UpdatableLLMEngine",
    "VLLM_AVAILABLE",
    "get_shared_state_dict_from_engine",
    "get_update_queue_from_engine",
    "init_weight_updater",
    "update_weights_from_file",
    "shutdown_updater",
]

__version__ = "0.1.0"
