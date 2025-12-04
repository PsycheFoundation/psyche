import torch
import torch.distributed as dist
import numpy as np
from typing import Dict, Optional
import logging

logger = logging.getLogger(__name__)

DTYPE_TO_ID = {
    torch.float32: 0,
    torch.float16: 1,
    torch.bfloat16: 2,
    torch.float64: 3,
    torch.int64: 4,
    torch.int32: 5,
    torch.int16: 6,
    torch.int8: 7,
    torch.uint8: 8,
}


# Broadcast a single parameter
# Broadcast metadata: [shutdown_flag, param_name_len, ndim, dim0, dim1, ..., dtype_id]
# Then broadcast tensor data
def broadcast_parameter(
    param_name: str,
    param_tensor: torch.Tensor,
    src_rank: int = 0,
    group=None,
):
    device = param_tensor.device

    param_name_bytes = param_name.encode("utf-8")
    param_name_len = len(param_name_bytes)

    if param_name_len > 100:
        raise ValueError(f"Parameter name too long: {len(param_name)} bytes (max 100)")

    # Metadata tensor format: [shutdown_flag=0, param_name_len, name_bytes..., ndim, dim0, dim1, ..., dtype_id]
    metadata = torch.zeros(128, dtype=torch.long, device=device)

    metadata[0] = 0

    metadata[1] = param_name_len

    # Parameter name bytes
    param_name_array = np.frombuffer(param_name_bytes, dtype=np.uint8).copy()
    metadata[2 : 2 + param_name_len] = (
        torch.from_numpy(param_name_array).long().to(device)
    )

    # Number of dimensions
    metadata[2 + param_name_len] = len(param_tensor.shape)

    # Shape dimensions
    for i, dim in enumerate(param_tensor.shape):
        metadata[3 + param_name_len + i] = dim

    # Dtype ID
    if param_tensor.dtype not in DTYPE_TO_ID:
        raise ValueError(f"Unsupported dtype: {param_tensor.dtype}")
    metadata[3 + param_name_len + len(param_tensor.shape)] = DTYPE_TO_ID[
        param_tensor.dtype
    ]

    dist.broadcast(metadata, src=src_rank, group=group)
    dist.broadcast(param_tensor, src=src_rank, group=group)


# Broadcast the entire state dict
def broadcast_state_dict(
    state_dict: Dict[str, torch.Tensor],
    src_rank: int = 0,
    group=None,
):
    for i, (param_name, param_tensor) in enumerate(state_dict.items()):
        broadcast_parameter(param_name, param_tensor, src_rank, group)

        if (i + 1) % 10 == 0:
            logger.info(f"Broadcasted {i + 1}/{len(state_dict)} parameters")


# Send shutdown signal
def broadcast_shutdown_signal(
    src_rank: int = 0,
    device: Optional[torch.device] = None,
    group=None,
):
    if device is None:
        device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    metadata = torch.zeros(128, dtype=torch.long, device=device)
    metadata[0] = -1  # Shutdown flag

    dist.broadcast(metadata, src=src_rank, group=group)
