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

                # Get the device that vLLM is using
                # vLLM model weights are already on GPU
                first_tensor = next(iter(state_dict.values()))
                device = first_tensor.device

                logger.info(
                    f"Spawning weight updater on device {device} with NCCL backend"
                )

                # Local process group: broadcaster (rank 0) + updater (rank 1)
                # Use NCCL for fast GPU-to-GPU communication (avoids GPU→CPU→GPU copies)
                # IMPORTANT: Don't set global NCCL env vars here as they would affect
                # vLLM's own NCCL groups (e.g., for multi-node tensor parallelism with IB)
                process_group_config = {
                    "backend": "nccl",
                    "init_method": "tcp://127.0.0.1:29500",
                    "world_size": 2,  # Always 2: local broadcaster + this updater
                    "rank": 1,  # Updater is always rank 1
                    "device": str(device),  # Pass device info to updater
                }

                ctx = mp.get_context("spawn")
                self.psyche_updater_process = ctx.Process(
                    target=weight_updater_process,
                    args=(state_dict, process_group_config, self.model_config),
                    daemon=True,
                )
                self.psyche_updater_process.start()

        import vllm.v1.worker.gpu_worker

        vllm.v1.worker.gpu_worker.GPUModelRunner = PsychePatchedGPUModelRunner

    except ImportError:
        pass


apply_vllm_patches()
