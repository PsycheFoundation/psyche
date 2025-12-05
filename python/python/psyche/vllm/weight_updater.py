"""
Simple weight updater process for vLLM.

Inspired by torchtitan's weight_updater_process but simplified for Psyche's use case:
- No torch.distributed (weights come from Iroh gossip/blobs handled by Rust)
- Uses a multiprocessing Queue to receive weight update requests
- Loads weights from safetensors and updates shared memory

The Rust inference node will:
1. Receive weight updates via Iroh gossip
2. Download blob and write to disk
3. Call `trigger_weight_update(path)` via PyO3
4. This process loads the weights from the queue
"""

import logging
import torch
import time
import os
from typing import Dict
from multiprocessing import Queue

logger = logging.getLogger(__name__)

# Global queue for weight update requests (set by patch after spawning updater)
_update_queue: Queue = None


def set_update_queue(queue: Queue):
    """Set the global update queue (called by vllm_patch after spawning updater)."""
    global _update_queue
    _update_queue = queue


def trigger_weight_update(safetensors_path: str):
    """
    Trigger a weight update from the main process.

    This function is called by Rust (via PyO3) to signal new weights are available.

    Args:
        safetensors_path: Path to the safetensors file containing new weights
    """
    global _update_queue
    if _update_queue is None:
        logger.error("[WeightUpdater] Queue not initialized, cannot trigger update")
        return

    logger.info(f"[WeightUpdater] Queuing weight update: {safetensors_path}")
    _update_queue.put(safetensors_path)


def weight_updater_process(state_dict: Dict[str, torch.Tensor], update_queue: Queue):
    """
    Weight updater process that receives weight update requests via queue.

    This process runs in parallel with vLLM inference and updates weights when requested.

    Args:
        state_dict: Shared memory state_dict from vLLM model
        update_queue: Queue to receive weight update paths
    """
    logger.info(f"[WeightUpdater] Process started (PID: {os.getpid()})")
    logger.info(f"[WeightUpdater] State dict has {len(state_dict)} parameters")

    # Get device from state_dict
    my_device = list(state_dict.values())[0].device
    logger.info(f"[WeightUpdater] Using device: {my_device}")

    update_count = 0

    with torch.no_grad():
        while True:
            try:
                # Block waiting for update request (timeout to allow checking for shutdown)
                try:
                    weights_path = update_queue.get(timeout=1.0)
                except:
                    # Timeout, continue loop
                    continue

                # Check for shutdown signal
                if weights_path is None or weights_path == "SHUTDOWN":
                    logger.info("[WeightUpdater] Received shutdown signal")
                    break

                if not os.path.exists(weights_path):
                    logger.warning(
                        f"[WeightUpdater] Weights file not found: {weights_path}"
                    )
                    continue

                logger.info(f"[WeightUpdater] Loading weights from: {weights_path}")

                # Load weights from safetensors
                from safetensors import safe_open

                loaded_count = 0
                skipped_count = 0

                with safe_open(weights_path, framework="pt", device="cpu") as f:
                    for key in f.keys():
                        if key in state_dict:
                            tensor = f.get_tensor(key)
                            # Verify shape matches
                            if tensor.shape != state_dict[key].shape:
                                logger.warning(
                                    f"[WeightUpdater] Shape mismatch for {key}: "
                                    f"expected {state_dict[key].shape}, got {tensor.shape}"
                                )
                                skipped_count += 1
                                continue

                            # Copy to device and update shared memory
                            state_dict[key].data.copy_(
                                tensor.to(device=my_device, dtype=state_dict[key].dtype)
                            )
                            loaded_count += 1
                        else:
                            logger.debug(
                                f"[WeightUpdater] Parameter {key} not in state_dict"
                            )
                            skipped_count += 1

                update_count += 1
                logger.info(
                    f"[WeightUpdater] ✓ Update #{update_count}: "
                    f"Loaded {loaded_count} parameters, skipped {skipped_count}"
                )

            except KeyboardInterrupt:
                logger.info("[WeightUpdater] Received interrupt, shutting down")
                break
            except Exception as e:
                logger.error(f"[WeightUpdater] Error: {e}")
                import traceback

                traceback.print_exc()
                time.sleep(1)  # Wait before retrying after error

    logger.info(
        f"[WeightUpdater] Process exiting: applied {update_count} weight updates"
    )
