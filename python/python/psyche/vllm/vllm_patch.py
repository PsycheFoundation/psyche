import logging
from typing import Dict, Optional, Any
import torch
import torch.multiprocessing as mp
import multiprocessing as mp_stdlib

logger = logging.getLogger(__name__)

# Use a multiprocessing Manager to share the state_dict reference across processes
# This allows the main process to access the dict that was created in the worker subprocess
_manager = mp_stdlib.Manager()
_shared_state_dict_proxy = _manager.dict()


def get_shared_state_dict() -> Optional[Dict[str, torch.Tensor]]:
    """
    Get the shared state_dict that was stored by the patched vLLM model runner.

    Returns:
        The shared memory state_dict if available, None otherwise
    """
    if len(_shared_state_dict_proxy) == 0:
        return None
    # Convert proxy dict back to regular dict
    return dict(_shared_state_dict_proxy)


def set_shared_state_dict(state_dict: Dict[str, torch.Tensor]):
    """
    Store the shared state_dict reference (called from patched load_model).

    Args:
        state_dict: The shared memory state_dict from vLLM model
    """
    _shared_state_dict_proxy.clear()
    _shared_state_dict_proxy.update(state_dict)
    logger.info(
        f"Stored shared state_dict with {len(state_dict)} parameters in manager"
    )


def apply_vllm_patches():
    try:
        from vllm.v1.worker.gpu_worker import GPUModelRunner

        logger.info("Psyche: Applying vLLM patches for weight access")

        class PsychePatchedGPUModelRunner(GPUModelRunner):
            def load_model(self, eep_scale_up: bool = False) -> None:
                logger.info("🔧 Psyche: Patched load_model() called!")
                logger.info("🔧 Psyche: Calling original GPUModelRunner.load_model()")
                super().load_model(eep_scale_up)
                logger.info("🔧 Psyche: Original load_model() completed")

                # Share model memory for weight updates
                logger.info("🔧 Psyche: Sharing model memory")
                self.model.share_memory()

                state_dict = self.model.state_dict()
                logger.info(
                    f"🔧 Psyche: Got state_dict with {len(state_dict)} parameters"
                )

                for key, val in state_dict.items():
                    if isinstance(val, torch.Tensor):
                        val.share_memory_()

                # Store in module-level variable for access from main process
                # The shared memory tensors can be accessed across processes
                set_shared_state_dict(state_dict)
                logger.info(
                    f"🔧 Psyche: Successfully stored shared state_dict with {len(state_dict)} parameters"
                )

        import vllm.v1.worker.gpu_worker

        vllm.v1.worker.gpu_worker.GPUModelRunner = PsychePatchedGPUModelRunner
        logger.info("✓ vLLM patch applied successfully")

    except ImportError as e:
        logger.warning(f"Could not apply vLLM patches: {e}")


apply_vllm_patches()
