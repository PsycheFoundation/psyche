import logging
import torch
import torch.distributed as dist
import numpy as np
from typing import Dict, Any

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

DTYPE_TO_ID = {v: k for k, v in DTYPE_MAP.items()}


def permute_for_rotary(weight: torch.Tensor, n_heads: int) -> torch.Tensor:
    if weight.dim() == 2:
        dim1, dim2 = weight.shape
        return (
            weight.view(n_heads, dim1 // n_heads // 2, 2, dim2)
            .transpose(1, 2)
            .reshape(dim1, dim2)
        )
    elif weight.dim() == 1:
        dim1 = weight.shape[0]
        return (
            weight.view(n_heads, dim1 // n_heads // 2, 2).transpose(1, 2).reshape(dim1)
        )
    else:
        logger.warning(
            f"Unexpected weight dimension for rotary permutation: {weight.dim()}"
        )
        return weight


# Weight updater process that receives updates via torch.distributed.
# Runs in a separate Python process and joins a torch.distributed process group.
# Receives weight updates and applies them to the shared memory state_dict.
def weight_updater_process(
    state_dict: Dict[str, torch.Tensor],
    process_group_config: Dict[str, Any],
    model_config: Any,
):
    backend = process_group_config.get("backend", "nccl")
    init_method = process_group_config.get("init_method")
    world_size = process_group_config.get("world_size")
    rank = process_group_config.get("rank")

    logger.info(
        f"Starting weight updater process: rank={rank}, world_size={world_size}, backend={backend}"
    )

    import os

    if init_method == "env://":
        if "MASTER_ADDR" not in os.environ or "MASTER_PORT" not in os.environ:
            raise RuntimeError("MASTER_ADDR/MASTER_PORT not set for env:// init")

    dist.init_process_group(
        backend=backend,
        init_method=init_method,
        world_size=world_size,
        rank=rank,
    )
    logger.info(f"Updater process group initialized (rank {rank}/{world_size})")

    my_device = list(state_dict.values())[0].device

    with torch.no_grad():
        try:
            update_count = 0
            applied_count = 0

            while True:
                # Metadata header format: [shutdown_flag, param_name_len, ndim, dim0, dim1, ..., dtype_id, ...]
                metadata = torch.zeros(128, dtype=torch.long, device=my_device)

                logger.debug(f"Waiting for metadata broadcast #{update_count + 1}...")
                dist.broadcast(metadata, src=0)

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
                param_tensor = torch.zeros(shape, dtype=dtype, device=my_device)
                dist.broadcast(param_tensor, src=0)

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
