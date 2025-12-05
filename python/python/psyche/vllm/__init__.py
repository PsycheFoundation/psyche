from . import vllm_patch  # Apply patches on import
from .engine import UpdatableLLMEngine, VLLM_AVAILABLE

_global_engine = None


def init_engine(model_name: str, **kwargs) -> UpdatableLLMEngine:
    global _global_engine
    _global_engine = UpdatableLLMEngine(model_name, **kwargs)
    return _global_engine


def get_engine() -> UpdatableLLMEngine:
    if _global_engine is None:
        raise RuntimeError("Engine not initialized. Call init_engine() first.")
    return _global_engine


def load_weights(safetensors_path: str):
    engine = get_engine()
    engine.load_weights(safetensors_path)


__all__ = [
    "UpdatableLLMEngine",
    "VLLM_AVAILABLE",
    "init_engine",
    "get_engine",
    "load_weights",
]

__version__ = "0.1.0"
