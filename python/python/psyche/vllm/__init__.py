from . import vllm_patch  # Apply patches on import
from .engine import UpdatableLLMEngine, VLLM_AVAILABLE
from .weight_updater import trigger_weight_update

_global_engine = None


def init_engine(model_name: str, **kwargs) -> UpdatableLLMEngine:
    global _global_engine
    _global_engine = UpdatableLLMEngine(model_name, **kwargs)
    return _global_engine


def get_engine() -> UpdatableLLMEngine:
    if _global_engine is None:
        raise RuntimeError("Engine not initialized. Call init_engine() first.")
    return _global_engine


__all__ = [
    "UpdatableLLMEngine",
    "VLLM_AVAILABLE",
    "init_engine",
    "get_engine",
    "trigger_weight_update",
]

__version__ = "0.1.0"
