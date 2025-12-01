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


def get_state_dict_from_engine(engine) -> Optional[Dict[str, torch.Tensor]]:
    """
    Get the shared state_dict from a vLLM engine via RPC.

    This works with vLLM 0.11+ by calling into the worker process
    to retrieve the psyche_shared_state_dict attribute.

    Args:
        engine: The vLLM LLMEngine instance

    Returns:
        The shared state_dict, or None if not available
    """
    try:
        # Call the custom method we added to Worker
        logger.info("Calling collective_rpc with 'get_psyche_state_dict'...")
        results = engine.collective_rpc("get_psyche_state_dict")
        logger.info(f"collective_rpc returned {len(results)} results")

        if results and results[0] is not None:
            logger.info(
                f"Successfully retrieved state_dict with {len(results[0])} parameters"
            )
            return results[0]
        else:
            logger.warning(f"collective_rpc returned empty or None: {results}")

    except Exception as e:
        logger.error(f"Error getting state_dict via RPC: {e}")
        import traceback

        traceback.print_exc()

    return None


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
        from vllm.v1.worker.gpu_worker import GPUModelRunner, Worker

        logger.info("Psyche: Applying vLLM patches for weight update support")

        # Patch Worker to add our custom methods
        class PsychePatchedWorker(Worker):
            """Psyche-patched Worker with weight update methods"""

            def get_psyche_param_names(self):
                """Get list of all parameter names."""
                if not hasattr(self, "model_runner") or not hasattr(
                    self.model_runner, "psyche_shared_state_dict"
                ):
                    return []

                state_dict = self.model_runner.psyche_shared_state_dict
                return list(state_dict.keys())

            def get_psyche_param_info(self, param_name: str):
                """Get shape and dtype for a parameter."""
                if not hasattr(self, "model_runner") or not hasattr(
                    self.model_runner, "psyche_shared_state_dict"
                ):
                    return None

                state_dict = self.model_runner.psyche_shared_state_dict
                if param_name in state_dict:
                    tensor = state_dict[param_name]
                    return {
                        "shape": list(tensor.shape),
                        "dtype": str(tensor.dtype),
                    }
                return None

            def update_psyche_weights(self, weight_updates):
                """
                Update weights in place using the shared state_dict.

                Args:
                    weight_updates: List of dicts with keys 'name', 'data', 'shape', 'dtype'
                """
                if not hasattr(self, "model_runner") or not hasattr(
                    self.model_runner, "psyche_shared_state_dict"
                ):
                    logger.error("psyche_shared_state_dict not available")
                    return False

                state_dict = self.model_runner.psyche_shared_state_dict
                updated_count = 0

                for item in weight_updates:
                    # Extract weight update info from dict
                    if isinstance(item, dict):
                        name = item.get("name")
                        data = item.get("data")
                        shape = item.get("shape")
                        dtype_str = item.get("dtype")
                    else:
                        logger.warning(f"Invalid weight update format: {type(item)}")
                        continue

                    if not name or data is None:
                        logger.warning("Missing name or data in weight update")
                        continue

                    if name in state_dict:
                        # Reconstruct tensor from flat list and metadata
                        try:
                            # Convert dtype string to torch dtype
                            if dtype_str:
                                dtype = getattr(torch, dtype_str.replace("torch.", ""))
                            else:
                                dtype = state_dict[name].dtype

                            # Create tensor from flat data list
                            import numpy as np

                            new_weight = torch.from_numpy(
                                np.array(data, dtype=np.float32)
                            )

                            # Reshape to target shape
                            if shape:
                                new_weight = new_weight.reshape(shape)

                            # Convert to correct dtype and device
                            new_weight = new_weight.to(
                                device=state_dict[name].device, dtype=dtype
                            )

                            # Verify shape matches
                            if new_weight.shape != state_dict[name].shape:
                                logger.error(
                                    f"Shape mismatch for {name}: "
                                    f"expected {state_dict[name].shape}, got {new_weight.shape}"
                                )
                                continue

                            # Update in place
                            state_dict[name].data.copy_(new_weight)
                            updated_count += 1
                        except Exception as e:
                            logger.error(f"Failed to update {name}: {e}")
                            import traceback

                            traceback.print_exc()
                            continue
                    else:
                        logger.warning(f"Parameter {name} not found in state_dict")

                logger.info(f"Updated {updated_count}/{len(weight_updates)} weights")
                return True

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

                    # Get model info for updater process
                    try:
                        from vllm.distributed import get_tensor_model_parallel_rank

                        worker_id = get_tensor_model_parallel_rank()
                    except ImportError:
                        worker_id = 0

                    # Register in global registry for fallback/debug access
                    register_shared_state_dict(
                        worker_id=worker_id,
                        state_dict=state_dict,
                        model_config=self.model_config,
                    )

                    logger.info(
                        f"Psyche: Successfully shared state_dict for worker {worker_id} "
                        f"with {len(state_dict)} parameters"
                    )

                    # Check if distributed updater should be spawned
                    # This is controlled by environment variable or can be done programmatically
                    import os

                    if os.environ.get("PSYCHE_USE_DISTRIBUTED_UPDATER") == "1":
                        logger.info(
                            "Psyche: Spawning distributed weight updater process"
                        )
                        self._spawn_distributed_updater(state_dict, worker_id)

                except Exception as e:
                    logger.error(f"Psyche: Failed to set up distributed updates: {e}")
                    raise

            def _spawn_distributed_updater(self, state_dict, worker_id):
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

        # Replace the classes with our patched versions
        import vllm.v1.worker.gpu_worker

        vllm.v1.worker.gpu_worker.Worker = PsychePatchedWorker
        vllm.v1.worker.gpu_worker.GPUModelRunner = PsychePatchedGPUModelRunner

        logger.info("Psyche: Successfully patched vLLM Worker and GPUModelRunner")

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
