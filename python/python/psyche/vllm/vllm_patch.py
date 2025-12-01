"""
vLLM Monkey Patching for Psyche Integration

This module patches vLLM's GPUModelRunner to enable direct access to the model's
state_dict for efficient weight updates. This approach is inspired by torchtitan's
GRPO implementation.

IMPORTANT: This module must be imported BEFORE any vLLM modules are imported.
"""

import logging
from typing import Dict, Optional, Any
import torch

logger = logging.getLogger(__name__)

# Global registry to store shared state_dicts from vLLM workers
_SHARED_STATE_DICTS: Dict[int, Dict[str, torch.Tensor]] = {}
_MODEL_CONFIG_REGISTRY: Dict[int, Any] = {}


def register_shared_state_dict(
    worker_id: int, state_dict: Dict[str, torch.Tensor], model_config: Any = None
):
    """
    Register a shared state_dict from a vLLM worker.

    Args:
        worker_id: Unique identifier for this worker (e.g., GPU rank)
        state_dict: The model's state_dict with shared memory tensors
        model_config: Optional model configuration for transformation metadata
    """
    _SHARED_STATE_DICTS[worker_id] = state_dict
    if model_config is not None:
        _MODEL_CONFIG_REGISTRY[worker_id] = model_config
    logger.info(
        f"Registered shared state_dict for worker {worker_id} with {len(state_dict)} parameters"
    )


def get_shared_state_dict(worker_id: int = 0) -> Optional[Dict[str, torch.Tensor]]:
    """
    Get the shared state_dict for a worker.

    Args:
        worker_id: Worker ID to retrieve (default: 0 for single-GPU case)

    Returns:
        The shared state_dict, or None if not registered
    """
    return _SHARED_STATE_DICTS.get(worker_id)


def get_all_shared_state_dicts() -> Dict[int, Dict[str, torch.Tensor]]:
    """Get all registered shared state_dicts."""
    return _SHARED_STATE_DICTS


def clear_registry():
    """Clear all registered state_dicts."""
    _SHARED_STATE_DICTS.clear()
    _MODEL_CONFIG_REGISTRY.clear()


def apply_vllm_patches():
    """
    Apply all vLLM patches for Psyche integration.

    This function must be called BEFORE any vLLM modules are imported.
    """
    try:
        from vllm.v1.worker.gpu_worker import GPUModelRunner

        logger.info("Psyche: Applying vLLM patches for weight update support")

        # Create patched class that inherits from GPUModelRunner
        # This is the same approach torchtitan uses
        class PsychePatchedGPUModelRunner(GPUModelRunner):
            """Psyche-patched GPUModelRunner with shared memory support"""

            def load_model(self, eep_scale_up: bool = False) -> None:
                # Call original load_model
                logger.info("Psyche: Calling original GPUModelRunner.load_model()")
                super().load_model(eep_scale_up)

                # Now expose shared memory access
                logger.info("Psyche: Sharing model memory for weight updates")
                try:
                    # Share model memory (like torchtitan does)
                    self.model.share_memory()

                    # Get state_dict and share all tensors
                    state_dict = self.model.state_dict()
                    for key, val in state_dict.items():
                        if isinstance(val, torch.Tensor):
                            val.share_memory_()

                    # Store on self for access
                    self.psyche_shared_state_dict = state_dict

                    # Also register in global registry for fallback access
                    try:
                        from vllm.distributed import get_tensor_model_parallel_rank

                        worker_id = get_tensor_model_parallel_rank()
                    except ImportError:
                        worker_id = 0

                    register_shared_state_dict(
                        worker_id=worker_id,
                        state_dict=state_dict,
                        model_config=self.model_config,
                    )

                    logger.info(
                        f"Psyche: Successfully shared state_dict for worker {worker_id} "
                        f"with {len(state_dict)} parameters"
                    )

                except Exception as e:
                    logger.error(f"Psyche: Failed to share model memory: {e}")
                    raise

        # Replace the GPUModelRunner class with our patched version
        import vllm.v1.worker.gpu_worker

        vllm.v1.worker.gpu_worker.GPUModelRunner = PsychePatchedGPUModelRunner

        logger.info("Psyche: Successfully patched vLLM GPUModelRunner")

    except ImportError as e:
        logger.warning(
            f"Psyche: Could not apply vLLM patches (vLLM not installed or incompatible): {e}"
        )
    except Exception as e:
        logger.error(f"Psyche: Failed to apply vLLM patches: {e}")
        raise


# Auto-apply patches when this module is imported
# This ensures patches are applied before vLLM engines are created
try:
    apply_vllm_patches()
except Exception as e:
    logger.warning(f"Failed to auto-apply vLLM patches: {e}")
