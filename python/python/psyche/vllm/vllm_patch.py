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


def get_update_queue_from_engine(engine):
    try:
        if hasattr(engine, "model_executor"):
            if hasattr(engine.model_executor, "driver_worker"):
                worker = engine.model_executor.driver_worker
                if hasattr(worker, "model_runner"):
                    model_runner = worker.model_runner
                    if hasattr(model_runner, "psyche_update_queue"):
                        return model_runner.psyche_update_queue

        logger.warning("Could not access psyche_update_queue from engine")
        return None
    except Exception as e:
        logger.error(f"Error accessing update queue: {e}")
        return None


def apply_vllm_patches():
    try:
        from vllm.v1.worker.gpu_worker import GPUModelRunner

        logger.info("Psyche: Applying vLLM patches for distributed weight updates")

        class PsychePatchedGPUModelRunner(GPUModelRunner):
            def load_model(self, eep_scale_up: bool = False) -> None:
                super().load_model(eep_scale_up)

                self.model.share_memory()

                state_dict = self.model.state_dict()
                for key, val in state_dict.items():
                    if isinstance(val, torch.Tensor):
                        val.share_memory_()

                self.psyche_shared_state_dict = state_dict

                self._spawn_distributed_updater(state_dict)

            def _spawn_distributed_updater(self, state_dict):
                from psyche.vllm.distributed_updater import weight_updater_process
                from psyche.vllm import engine as engine_mod

                # Get the device that vLLM is using
                first_tensor = next(iter(state_dict.values()))
                device = first_tensor.device

                logger.info(f"Spawning weight updater on device {device}")

                # Get the queue from the engine module
                # It's a Manager().Queue() created in UpdatableLLMEngine.__init__
                if engine_mod._current_update_queue is None:
                    logger.error(
                        "No update queue found - was UpdatableLLMEngine created?"
                    )
                    return

                update_queue = engine_mod._current_update_queue
                logger.info("Got update queue from engine module")

                ctx = mp.get_context("spawn")
                self.psyche_updater_process = ctx.Process(
                    target=weight_updater_process,
                    args=(state_dict, update_queue, self.model_config),
                    daemon=True,
                )
                self.psyche_updater_process.start()

                logger.info(
                    "Weight updater process started, waiting for checkpoint updates"
                )

        import vllm.v1.worker.gpu_worker

        vllm.v1.worker.gpu_worker.GPUModelRunner = PsychePatchedGPUModelRunner

    except ImportError:
        pass


apply_vllm_patches()
