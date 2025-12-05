import logging
from typing import Dict, Optional, Any
import torch
import torch.multiprocessing as mp

logger = logging.getLogger(__name__)


# This method gets the state_dict from our vLLM engine for verification.
# Used for testing that the shared memory mechanism works.
def get_shared_state_dict_from_engine(engine) -> Optional[Dict[str, torch.Tensor]]:
    try:
        logger.debug(f"Engine type: {type(engine)}")
        logger.debug(f"Has model_executor: {hasattr(engine, 'model_executor')}")

        if hasattr(engine, "model_executor"):
            logger.debug(f"model_executor type: {type(engine.model_executor)}")
            logger.debug(
                f"Has driver_worker: {hasattr(engine.model_executor, 'driver_worker')}"
            )

            if hasattr(engine.model_executor, "driver_worker"):
                worker = engine.model_executor.driver_worker
                logger.debug(f"driver_worker type: {type(worker)}")
                logger.debug(f"Has model_runner: {hasattr(worker, 'model_runner')}")

                if hasattr(worker, "model_runner"):
                    model_runner = worker.model_runner
                    logger.debug(f"model_runner type: {type(model_runner)}")
                    logger.debug(
                        f"Has psyche_shared_state_dict: {hasattr(model_runner, 'psyche_shared_state_dict')}"
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
                super().load_model(eep_scale_up)

                # Share model memory for weight updates
                self.model.share_memory()

                state_dict = self.model.state_dict()
                for key, val in state_dict.items():
                    if isinstance(val, torch.Tensor):
                        val.share_memory_()

                # Store state_dict reference for Psyche weight updates
                self.psyche_shared_state_dict = state_dict
                logger.info(
                    f"Stored psyche_shared_state_dict with {len(state_dict)} parameters"
                )

        import vllm.v1.worker.gpu_worker

        vllm.v1.worker.gpu_worker.GPUModelRunner = PsychePatchedGPUModelRunner
        logger.info("✓ vLLM patch applied successfully")

    except ImportError as e:
        logger.warning(f"Could not apply vLLM patches: {e}")


apply_vllm_patches()
