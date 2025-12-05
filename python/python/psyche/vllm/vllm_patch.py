import logging
import os
from typing import Dict, Optional, Any
import torch
import torch.multiprocessing as mp

logger = logging.getLogger(__name__)

# Global queue reference
_global_update_queue = None


def get_update_queue():
    return _global_update_queue


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

                # Spawn weight updater process
                # Check if updater is enabled
                enable_updater = os.environ.get("PSYCHE_ENABLE_WEIGHT_UPDATER", "1")
                if enable_updater == "1":
                    logger.info("Psyche: Spawning weight updater process")

                    from psyche.vllm.weight_updater import weight_updater_process

                    # Create queue for weight update requests
                    ctx = mp.get_context("spawn")
                    update_queue = ctx.Queue()

                    # Spawn updater process
                    self.psyche_updater_process = ctx.Process(
                        target=weight_updater_process,
                        args=(state_dict, update_queue),
                        daemon=True,
                    )
                    self.psyche_updater_process.start()

                    global _global_update_queue
                    _global_update_queue = update_queue

                    logger.info(
                        f"Psyche: Weight updater process started (PID: {self.psyche_updater_process.pid})"
                    )
                else:
                    logger.info(
                        "Psyche: PSYCHE_ENABLE_WEIGHT_UPDATER=0, skipping updater spawn"
                    )

        import vllm.v1.worker.gpu_worker

        vllm.v1.worker.gpu_worker.GPUModelRunner = PsychePatchedGPUModelRunner
        logger.info("vLLM patch applied successfully")

    except ImportError as e:
        logger.warning(f"Could not apply vLLM patches: {e}")


apply_vllm_patches()
