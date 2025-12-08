"""
Bridge API for Rust to trigger weight updates on inference nodes.

This module provides a simple function that Rust calls via PyO3 to copy
checkpoint files to the update directory where vLLM watches for them.
"""

import logging
from pathlib import Path
import shutil
import time

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
    Copy checkpoint to the update directory for the updater to process.

    This is the main function that Rust calls via PyO3 after downloading
    a new checkpoint from iroh. The file is copied to the watched directory
    with a timestamp-based name.

    Args:
        checkpoint_path: Path to the safetensors checkpoint file
    """
    from .engine import CHECKPOINT_DIR

    # Verify file exists
    source_path = Path(checkpoint_path)
    if not source_path.exists():
        raise FileNotFoundError(f"Checkpoint file not found: {checkpoint_path}")

    logger.info(f"Copying checkpoint to update directory: {checkpoint_path}")

    # Create unique filename with timestamp
    timestamp = int(time.time() * 1000000)  # microseconds
    final_path = CHECKPOINT_DIR / f"checkpoint_{timestamp}.safetensors"
    temp_path = CHECKPOINT_DIR / f".tmp_{timestamp}.safetensors"

    # Copy to temp file first, then atomically rename
    # This prevents the updater from seeing incomplete files
    shutil.copy2(source_path, temp_path)
    temp_path.rename(final_path)

    logger.info(f"✓ Checkpoint copied to {final_path}")


def shutdown_updater() -> None:
    """
    Send shutdown signal to the updater subprocess.

    Call this when shutting down the inference node.
    """
    from .engine import CHECKPOINT_DIR

    logger.info("Sending shutdown signal to updater subprocess...")

    # Create shutdown signal file
    shutdown_file = CHECKPOINT_DIR / "SHUTDOWN"
    shutdown_file.touch()

    logger.info("✓ Shutdown signal sent")
