import logging
import torch
import torch.distributed as dist
import numpy as np
from typing import Dict, Any, Optional
from datetime import timedelta

logger = logging.getLogger(__name__)


DTYPE_MAP = {
    0: torch.float32,
    1: torch.float16,
    2: torch.bfloat16,
    3: torch.float64,
    4: torch.int64,
    5: torch.int32,
    6: torch.int16,
    7: torch.int8,
    8: torch.uint8,
}


def init_process_group(
    backend=None,
    init_method: Optional[str] = None,
    timeout: Optional[timedelta] = None,
    world_size: int = -1,
    rank: int = -1,
    store: Optional = None,
    group_name: str = None,
    pg_options: Optional[Any] = None,
):
    from torch.distributed.distributed_c10d import (
        _new_process_group_helper,
        _world,
        Backend,
        default_pg_timeout,
        PrefixStore,
        rendezvous,
    )

    assert (store is None) or (
        init_method is None
    ), "Cannot specify both init_method and store."

    if store is not None:
        assert world_size > 0, "world_size must be positive if using store"
        assert rank >= 0, "rank must be non-negative if using store"
    elif init_method is None:
        init_method = "env://"

    if backend:
        backend = Backend(backend)
    else:
        backend = Backend("undefined")

    if timeout is None:
        timeout = default_pg_timeout

    # backward compatible API
    if store is None:
        rendezvous_iterator = rendezvous(init_method, rank, world_size, timeout=timeout)
        store, rank, world_size = next(rendezvous_iterator)
        store.set_timeout(timeout)
        store = PrefixStore(group_name, store)

    # The pg_options parameter was renamed into backend_options in PyTorch 2.6.0
    # https://github.com/pytorch/pytorch/commit/a0c7029a75628cd5fa8df83c0de0ea98ee7fd844
    # We need to determine the appropriate parameter name based on PyTorch version
    pg_options_param_name = (
        "backend_options" if str(torch.__version__) >= "2.6" else "pg_options"
    )
    pg, _ = _new_process_group_helper(
        world_size,
        rank,
        [],
        backend,
        store,
        group_name=group_name,
        **{pg_options_param_name: pg_options},
        timeout=timeout,
    )

    _world.pg_group_ranks[pg] = {i: i for i in range(world_size)}

    return pg


# Weight updater process that watches a directory for checkpoint files.
# Runs in a separate Python process and polls for new checkpoints.
# When found, loads checkpoint from disk and updates the shared memory state_dict.
def weight_updater_process(
    state_dict: Dict[str, torch.Tensor],
    checkpoint_dir: str,
    model_config: Any,
):
    from safetensors import safe_open
    import time
    from pathlib import Path

    # Get device from state_dict
    device = list(state_dict.values())[0].device

    logger.info(f"Starting weight updater process on device {device}")
    logger.info(f"Watching directory: {checkpoint_dir}")

    checkpoint_path = Path(checkpoint_dir)
    checkpoint_path.mkdir(parents=True, exist_ok=True)

    # Track which checkpoints we've already processed
    processed_checkpoints = set()

    with torch.no_grad():
        try:
            update_count = 0

            while True:
                # Check for shutdown signal file
                shutdown_file = checkpoint_path / "SHUTDOWN"
                if shutdown_file.exists():
                    logger.info("Received shutdown signal, exiting")
                    shutdown_file.unlink()
                    break

                # List all .safetensors files in directory (excluding temp files)
                checkpoint_files = sorted(
                    f
                    for f in checkpoint_path.glob("*.safetensors")
                    if not f.name.startswith(".tmp_")
                )

                # Find new checkpoints we haven't processed
                new_checkpoints = [
                    f for f in checkpoint_files if f not in processed_checkpoints
                ]

                if new_checkpoints:
                    # Process the oldest new checkpoint
                    checkpoint_file = new_checkpoints[0]
                    logger.info(f"Found new checkpoint: {checkpoint_file}")

                    try:
                        # Load checkpoint from disk
                        logger.info(f"Loading checkpoint from {checkpoint_file}")
                        with safe_open(
                            str(checkpoint_file), framework="pt", device=str(device)
                        ) as f:
                            applied_count = 0
                            for key in f.keys():
                                if key in state_dict:
                                    tensor = f.get_tensor(key)

                                    # Verify shape matches
                                    if state_dict[key].shape != tensor.shape:
                                        logger.error(
                                            f"Shape mismatch for {key}: "
                                            f"expected {state_dict[key].shape}, got {tensor.shape}"
                                        )
                                        continue

                                    # Copy to shared memory
                                    state_dict[key].copy_(tensor)
                                    applied_count += 1

                                    if applied_count % 100 == 0:
                                        logger.debug(
                                            f"Applied {applied_count} parameters..."
                                        )
                                else:
                                    logger.warning(
                                        f"Parameter {key} not in vLLM state_dict"
                                    )

                        update_count += 1
                        logger.info(
                            f"✓ Checkpoint {update_count} applied successfully "
                            f"({applied_count} parameters updated)"
                        )

                        # Mark as processed and delete the file
                        processed_checkpoints.add(checkpoint_file)
                        checkpoint_file.unlink()
                        logger.info(f"Deleted checkpoint file: {checkpoint_file}")

                    except Exception as e:
                        logger.error(
                            f"Failed to load checkpoint {checkpoint_file}: {e}"
                        )
                        import traceback

                        traceback.print_exc()
                        # Mark as processed even on error to avoid infinite retry
                        processed_checkpoints.add(checkpoint_file)

                # Sleep before checking again
                time.sleep(0.5)

        except KeyboardInterrupt:
            pass

    logger.info(f"Updater exiting: {update_count} checkpoints applied")
