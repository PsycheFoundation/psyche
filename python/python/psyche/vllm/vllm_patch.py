import logging
from typing import Dict, Optional, Any
import torch
import torch.multiprocessing as mp

logger = logging.getLogger(__name__)


# This method gets the state_dict from our vLLM engine for verification.
# Used for testing that the shared memory mechanism works.
def get_shared_state_dict_from_engine(engine) -> Optional[Dict[str, torch.Tensor]]:
    try:
        if hasattr(engine, "model_executor"):
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
    try:
        from vllm.v1.worker.gpu_worker import GPUModelRunner

        logger.info("Psyche: Applying vLLM patches for distributed weight updates")

        class PsychePatchedGPUModelRunner(GPUModelRunner):
            def load_model(self, eep_scale_up: bool = False) -> None:
                logger.info("Psyche: Calling original GPUModelRunner.load_model()")
                super().load_model(eep_scale_up)

                logger.info("Psyche: Setting up distributed weight updates")
                try:
                    self.model.share_memory()

                    state_dict = self.model.state_dict()
                    for key, val in state_dict.items():
                        if isinstance(val, torch.Tensor):
                            val.share_memory_()

                    self.psyche_shared_state_dict = state_dict

                    logger.info(
                        f"Psyche: Successfully shared state_dict "
                        f"with {len(state_dict)} parameters"
                    )

                    logger.info("Psyche: Spawning distributed weight updater process")
                    self._spawn_distributed_updater(state_dict)

                except Exception as e:
                    logger.error(f"Psyche: Failed to set up distributed updates: {e}")
                    raise

            def _spawn_distributed_updater(self, state_dict):
                try:
                    from psyche.vllm.distributed_updater import weight_updater_process
                    import os

                    if "PSYCHE_WORLD_SIZE" not in os.environ:
                        logger.info(
                            "Psyche: PSYCHE_WORLD_SIZE not set, skipping distributed updater spawn. "
                            "Set PSYCHE_WORLD_SIZE, PSYCHE_RANK, MASTER_ADDR, MASTER_PORT to enable distributed mode. "
                            "This is expected during basic testing without Psyche coordinator."
                        )
                        return

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

                    if process_group_config["init_method"] == "env://":
                        if (
                            "MASTER_ADDR" not in os.environ
                            or "MASTER_PORT" not in os.environ
                        ):
                            logger.warning(
                                "Psyche: init_method='env://' but MASTER_ADDR/MASTER_PORT not set. "
                                "Updater may fail to join process group."
                            )

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
                    logger.warning("Continuing without distributed updater")

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
try:
    apply_vllm_patches()
except Exception as e:
    logger.warning(f"Failed to auto-apply vLLM patches: {e}")
