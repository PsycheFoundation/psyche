import logging
import os
from typing import Dict, Optional, Any
import torch
import torch.multiprocessing as mp
from multiprocessing.managers import SyncManager

logger = logging.getLogger(__name__)

# Shared state manager for cross-process communication
_shared_manager = SyncManager()
_shared_manager.start()
_shared_state = _shared_manager.dict()
_shared_state["update_queue"] = None


def set_update_queue(queue):
    """Store the update queue in shared state (called from main process)."""
    _shared_state["update_queue"] = queue


def get_update_queue():
    """Get the update queue from shared state (works from any process)."""
    return _shared_state.get("update_queue", None)


def apply_vllm_patches():
    try:
        from vllm.v1.worker.gpu_worker import GPUModelRunner

        logger.info("Psyche: Applying vLLM patches for weight access")

        class PsychePatchedGPUModelRunner(GPUModelRunner):
            def load_model(self, eep_scale_up: bool = False) -> None:
                logger.info("Psyche: Patched load_model() called!")
                super().load_model(eep_scale_up)
                logger.info("Psyche: Original load_model() completed")

                logger.info("Psyche: Sharing model memory")
                self.model.share_memory()

                state_dict = self.model.state_dict()
                logger.info(f"Psyche: Got state_dict with {len(state_dict)} parameters")

                for key, val in state_dict.items():
                    if isinstance(val, torch.Tensor):
                        val.share_memory_()

                update_queue = get_update_queue()
                if update_queue is not None:
                    logger.info("Psyche: Spawning weight updater process")

                    from psyche.vllm.weight_updater import weight_updater_process

                    ctx = mp.get_context("spawn")
                    self.psyche_updater_process = ctx.Process(
                        target=weight_updater_process,
                        args=(state_dict, update_queue),
                        daemon=True,
                    )
                    self.psyche_updater_process.start()

                    logger.info(
                        f"Psyche: Weight updater process started (PID: {self.psyche_updater_process.pid})"
                    )
                else:
                    logger.info(
                        "Psyche: No update queue provided, skipping updater spawn"
                    )

        import vllm.v1.worker.gpu_worker

        vllm.v1.worker.gpu_worker.GPUModelRunner = PsychePatchedGPUModelRunner
        logger.info("vLLM patch applied successfully")

    except ImportError as e:
        logger.warning(f"Could not apply vLLM patches: {e}")


apply_vllm_patches()
