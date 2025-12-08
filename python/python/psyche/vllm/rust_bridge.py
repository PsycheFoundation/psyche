"""
Bridge API for Rust to trigger weight updates on inference nodes.

This module provides a simple function that Rust calls via PyO3 to notify
the vLLM updater subprocess about new checkpoint files.
"""

import logging
from pathlib import Path

logger = logging.getLogger(__name__)

# Global reference to the vLLM engine (set by init_weight_updater)
_vllm_engine = None


def init_weight_updater(vllm_engine) -> None:
    """
    Initialize the weight updater bridge with a reference to the vLLM engine.

    This should be called once after creating the vLLM engine.

    Args:
        vllm_engine: The UpdatableLLMEngine instance
    """
    global _vllm_engine
    _vllm_engine = vllm_engine
    logger.info("✓ Weight updater bridge initialized")


def update_weights_from_file(checkpoint_path: str) -> None:
    """
    Notify the updater subprocess about a new checkpoint file.

    This is the main function that Rust calls via PyO3 after downloading
    a new checkpoint from iroh. The path is sent to the updater via a queue,
    and the updater loads it asynchronously.

    Args:
        checkpoint_path: Path to the safetensors checkpoint file
    """
    if _vllm_engine is None:
        raise RuntimeError(
            "Weight updater not initialized. Call init_weight_updater() first."
        )

    # Verify file exists
    path = Path(checkpoint_path)
    if not path.exists():
        raise FileNotFoundError(f"Checkpoint file not found: {checkpoint_path}")

    logger.info(f"Queueing checkpoint update: {checkpoint_path}")

    if not hasattr(_vllm_engine, "_update_queue") or _vllm_engine._update_queue is None:
        raise RuntimeError("Update queue not initialized on engine")

    _vllm_engine._update_queue.put(str(checkpoint_path))

    logger.info(f"✓ Checkpoint update queued successfully")


def shutdown_updater() -> None:
    """
    Send shutdown signal to the updater subprocess.

    Call this when shutting down the inference node.
    """
    if _vllm_engine is None:
        logger.warning("Weight updater not initialized, nothing to shut down")
        return

    logger.info("Sending shutdown signal to updater subprocess...")

    from .vllm_patch import get_update_queue_from_engine

    queue = get_update_queue_from_engine(_vllm_engine.engine)
    if queue is not None:
        queue.put(None)  # None signals shutdown

    logger.info("✓ Shutdown signal sent")
