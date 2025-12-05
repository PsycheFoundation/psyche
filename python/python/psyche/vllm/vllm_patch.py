import logging
from typing import Dict, Optional, Any
import torch
import torch.multiprocessing as mp

logger = logging.getLogger(__name__)


# This method gets the state_dict from our vLLM engine for verification.
# Used for testing that the shared memory mechanism works.
def get_shared_state_dict_from_engine(engine) -> Optional[Dict[str, torch.Tensor]]:
    try:
        logger.info(f"[DEBUG] Engine type: {type(engine)}")
        logger.info(f"[DEBUG] Has model_executor: {hasattr(engine, 'model_executor')}")

        if hasattr(engine, "model_executor"):
            logger.info(f"[DEBUG] model_executor type: {type(engine.model_executor)}")
            logger.info(
                f"[DEBUG] Has driver_worker: {hasattr(engine.model_executor, 'driver_worker')}"
            )

            if hasattr(engine.model_executor, "driver_worker"):
                worker = engine.model_executor.driver_worker
                logger.info(f"[DEBUG] driver_worker type: {type(worker)}")
                logger.info(
                    f"[DEBUG] Has model_runner: {hasattr(worker, 'model_runner')}"
                )

                if hasattr(worker, "model_runner"):
                    model_runner = worker.model_runner
                    logger.info(f"[DEBUG] model_runner type: {type(model_runner)}")
                    logger.info(
                        f"[DEBUG] Has psyche_shared_state_dict: {hasattr(model_runner, 'psyche_shared_state_dict')}"
                    )

                    if hasattr(model_runner, "psyche_shared_state_dict"):
                        return model_runner.psyche_shared_state_dict

        logger.warning("Could not access psyche_shared_state_dict from engine")
        return None
    except Exception as e:
        logger.error(f"Error accessing shared state_dict: {e}")
        import traceback

        traceback.print_exc()
        return None


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

                # Store state_dict reference for Psyche weight updates
                self.psyche_shared_state_dict = state_dict
                logger.info(
                    f"🔧 Psyche: Successfully stored psyche_shared_state_dict with {len(state_dict)} parameters"
                )

        import vllm.v1.worker.gpu_worker

        vllm.v1.worker.gpu_worker.GPUModelRunner = PsychePatchedGPUModelRunner
        logger.info("✓ vLLM patch applied successfully")

    except ImportError as e:
        logger.warning(f"Could not apply vLLM patches: {e}")


apply_vllm_patches()
