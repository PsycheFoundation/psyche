"""
vLLM Monkey Patching for Psyche Integration

This module patches vLLM's GPUModelRunner to enable distributed weight updates
via torch.distributed. This approach is inspired by torchtitan's GRPO implementation.

IMPORTANT: This module must be imported BEFORE any vLLM modules are imported.
"""

import logging
from typing import Dict, Optional, Any
import torch
import torch.multiprocessing as mp

logger = logging.getLogger(__name__)

# Removed global registry - torch.distributed handles everything now


def get_shared_state_dict_from_engine(engine) -> Optional[Dict[str, torch.Tensor]]:
    """
    Get the shared state_dict from a vLLM engine for testing/debugging.

    In production, weight updates come via torch.distributed to the updater process.
    This function is for testing that the shared memory mechanism works.

    Args:
        engine: vLLM LLMEngine instance

    Returns:
        Shared state_dict if available, None otherwise
    """
    try:
        # Navigate to the GPUModelRunner through vLLM's architecture
        # Engine -> WorkerGroup -> workers -> model_runner
        if hasattr(engine, "model_executor"):
            # Try to get to the model runner
            if hasattr(engine.model_executor, "driver_worker"):
                worker = engine.model_executor.driver_worker
                if hasattr(worker, "model_runner"):
                    model_runner = worker.model_runner
                    if hasattr(model_runner, "psyche_shared_state_dict"):
                        return model_runner.psyche_shared_state_dict

        logger.warning("Could not access psyche_shared_state_dict from engine")
        return None
    except Exception as e:
        logger.error(f"Error accessing shared state_dict: {e}")
        return None


def apply_vllm_patches():
    """
    Apply all vLLM patches for Psyche integration.

    This function must be called BEFORE any vLLM modules are imported.
    """
    try:
        from vllm.v1.worker.gpu_worker import GPUModelRunner

        logger.info("Psyche: Applying vLLM patches for distributed weight updates")

        # Create patched class that inherits from GPUModelRunner
        # This is the same approach torchtitan uses
        class PsychePatchedGPUModelRunner(GPUModelRunner):
            """Psyche-patched GPUModelRunner with distributed updater support"""

            def load_model(self, eep_scale_up: bool = False) -> None:
                # Call original load_model
                logger.info("Psyche: Calling original GPUModelRunner.load_model()")
                super().load_model(eep_scale_up)

                # Now set up distributed weight updates
                logger.info("Psyche: Setting up distributed weight updates")
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

                    logger.info(
                        f"Psyche: Successfully shared state_dict "
                        f"with {len(state_dict)} parameters"
                    )

                    # Spawn distributed weight updater process
                    logger.info("Psyche: Spawning distributed weight updater process")
                    self._spawn_distributed_updater(state_dict)

                except Exception as e:
                    logger.error(f"Psyche: Failed to set up distributed updates: {e}")
                    raise

            def _spawn_distributed_updater(self, state_dict):
                """Spawn the distributed weight updater process"""
                try:
                    from psyche.vllm.distributed_updater import weight_updater_process
                    import os

                    # Get distributed config from environment
                    # These should be set by Psyche's distributed training infrastructure
                    process_group_config = {
                        "backend": os.environ.get("PSYCHE_UPDATER_BACKEND", "nccl"),
                        "init_method": os.environ.get(
                            "PSYCHE_UPDATER_INIT_METHOD", "env://"
                        ),
                        "world_size": int(os.environ.get("PSYCHE_WORLD_SIZE", 1)),
                        "rank": int(os.environ.get("PSYCHE_RANK", 0)),
                    }

                    logger.info(
                        f"Psyche: Spawning distributed updater with config: {process_group_config}"
                    )

                    # Spawn updater process
                    ctx = mp.get_context("spawn")

                    self.psyche_updater_process = ctx.Process(
                        target=weight_updater_process,
                        args=(state_dict, process_group_config, self.model_config),
                        daemon=True,
                    )

                    self.psyche_updater_process.start()

                    logger.info(
                        f"Psyche: Distributed updater process started (PID: {self.psyche_updater_process.pid})"
                    )

                except Exception as e:
                    logger.error(f"Psyche: Failed to spawn distributed updater: {e}")
                    import traceback

                    traceback.print_exc()
                    # Don't fail the entire model load if updater fails
                    logger.warning("Continuing without distributed updater")

        # Replace GPUModelRunner with our patched version
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
