from . import vllm_patch  # Apply patches on import
from .engine import UpdatableLLMEngine, VLLM_AVAILABLE


def init_engine(model_name: str, **kwargs) -> UpdatableLLMEngine:
    """
    Initialize a vLLM engine with weight update support.

    After calling this and running the first inference request (which triggers model loading),
    call engine.get_update_queue() to get the queue for triggering weight updates.

    Returns:
        UpdatableLLMEngine instance
    """
    return UpdatableLLMEngine(model_name, **kwargs)


__all__ = [
    "UpdatableLLMEngine",
    "VLLM_AVAILABLE",
    "init_engine",
]

__version__ = "0.1.0"
