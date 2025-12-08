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


# Weight updater process that receives updates via torch.distributed.
# Runs in a separate Python process and joins a torch.distributed process group.
# Receives weight updates and applies them to the shared memory state_dict.
def weight_updater_process(
    state_dict: Dict[str, torch.Tensor],
    process_group_config: Dict[str, Any],
    model_config: Any,
):
    backend = process_group_config["backend"]
    init_method = process_group_config["init_method"]
    world_size = process_group_config["world_size"]
    rank = process_group_config["rank"]
    device_str = process_group_config.get("device", "cuda:0")

    logger.info(
        f"Starting weight updater process: rank={rank}/{world_size}, backend={backend}, device={device_str}"
    )

    my_device = torch.device(device_str)
    if my_device.type == "cuda":
        torch.cuda.set_device(my_device)
        logger.info(f"Set CUDA device to {my_device}")

    vllm_group = init_process_group(
        backend=backend,
        init_method=init_method,
        world_size=world_size,
        rank=rank,
        group_name="vllm_updater",
    )
    logger.info(
        f"Updater joined vLLM process group (rank {rank}/{world_size}, backend={backend})"
    )

    # Verify device from state_dict matches
    actual_device = list(state_dict.values())[0].device
    logger.info(f"State dict tensors are on device: {actual_device}")

    comm_device = my_device if backend == "nccl" else actual_device

    with torch.no_grad():
        try:
            update_count = 0
            applied_count = 0

            while True:
                # Metadata header format: [shutdown_flag, param_name_len, ndim, dim0, dim1, ..., dtype_id, ...]
                metadata = torch.zeros(128, dtype=torch.long, device=comm_device)

                logger.debug(f"Waiting for metadata broadcast #{update_count + 1}...")
                dist.broadcast(metadata, src=0, group=vllm_group)

                shutdown_flag = metadata[0].item()
                if shutdown_flag == -1:
                    logger.info("Received shutdown signal, exiting")
                    break

                param_name_len = int(metadata[1].item())
                if param_name_len == 0 or param_name_len > 100:
                    logger.error(f"Invalid param_name_len: {param_name_len}")
                    break

                param_name_bytes = (
                    metadata[2 : 2 + param_name_len]
                    .cpu()
                    .numpy()
                    .astype(np.uint8)
                    .tobytes()
                )
                param_name = param_name_bytes.decode("utf-8").rstrip("\x00")

                # Extract shape
                ndim = int(metadata[2 + param_name_len].item())
                shape = tuple(
                    int(metadata[3 + param_name_len + i].item()) for i in range(ndim)
                )

                # Extract dtype
                dtype_id = int(metadata[3 + param_name_len + ndim].item())
                if dtype_id not in DTYPE_MAP:
                    logger.error(f"Unknown dtype_id: {dtype_id}")
                    continue
                dtype = DTYPE_MAP[dtype_id]

                logger.info(
                    f"Receiving parameter: {param_name} (shape={shape}, dtype={dtype})"
                )

                # Parameter tensor
                param_tensor = torch.zeros(shape, dtype=dtype, device=comm_device)
                dist.broadcast(param_tensor, src=0, group=vllm_group)

                update_count += 1

                if param_name in state_dict:
                    if state_dict[param_name].shape != param_tensor.shape:
                        logger.error(
                            f"Shape mismatch for {param_name}: "
                            f"expected {state_dict[param_name].shape}, got {param_tensor.shape}"
                        )
                        continue

                    state_dict[param_name].data.copy_(param_tensor)
                    applied_count += 1

                    if applied_count % 10 == 0:
                        logger.info(f"Applied {applied_count} parameter updates")
                else:
                    logger.warning(f"Parameter {param_name} not in state_dict")

        except KeyboardInterrupt:
            pass

    logger.info(f"Updater exiting: {applied_count}/{update_count} updates applied")
    dist.destroy_process_group()
