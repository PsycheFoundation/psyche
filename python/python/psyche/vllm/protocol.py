"""
Protocol for broadcasting parameters from coordinator to inference nodes.

This module provides utilities for encoding and sending model parameters
via torch.distributed broadcast operations.
"""

import torch
import torch.distributed as dist
import numpy as np
from typing import Dict, Optional
import logging

logger = logging.getLogger(__name__)


# Dtype mapping (must match distributed_updater.py)
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


def broadcast_parameter(
    param_name: str,
    param_tensor: torch.Tensor,
    src_rank: int = 0,
    group=None,
):
    """
    Broadcast a single parameter with its metadata.

    Protocol:
    1. Broadcast metadata: [shutdown_flag, param_name_len, ndim, dim0, dim1, ..., dtype_id]
    2. Broadcast tensor data

    Args:
        param_name: Name of parameter (e.g., "model.layers.0.self_attn.q_proj.weight")
        param_tensor: Parameter tensor to broadcast
        src_rank: Source rank for broadcast (default: 0)
        group: Process group (default: None = all processes)
    """
    device = param_tensor.device

    # Encode parameter name
    param_name_bytes = param_name.encode("utf-8")
    param_name_len = len(param_name_bytes)

    if param_name_len > 100:
        raise ValueError(f"Parameter name too long: {len(param_name)} bytes (max 100)")

    # Create metadata tensor
    # Format: [shutdown_flag=0, param_name_len, name_bytes..., ndim, dim0, dim1, ..., dtype_id]
    metadata = torch.zeros(128, dtype=torch.long, device=device)

    # Shutdown flag (0 = not shutdown)
    metadata[0] = 0

    # Parameter name length
    metadata[1] = param_name_len

    # Parameter name bytes
    param_name_array = np.frombuffer(param_name_bytes, dtype=np.uint8)
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

    # Broadcast metadata
    logger.debug(f"Broadcasting metadata for {param_name}")
    dist.broadcast(metadata, src=src_rank, group=group)

    # Broadcast tensor
    logger.debug(f"Broadcasting tensor for {param_name} (shape={param_tensor.shape})")
    dist.broadcast(param_tensor, src=src_rank, group=group)

    logger.debug(f"✓ Broadcasted {param_name}")


def broadcast_state_dict(
    state_dict: Dict[str, torch.Tensor],
    src_rank: int = 0,
    group=None,
):
    """
    Broadcast entire state_dict.

    Args:
        state_dict: Dictionary of parameter name -> tensor
        src_rank: Source rank for broadcast (default: 0)
        group: Process group (default: None = all processes)
    """
    logger.info(f"Broadcasting {len(state_dict)} parameters")

    for i, (param_name, param_tensor) in enumerate(state_dict.items()):
        broadcast_parameter(param_name, param_tensor, src_rank, group)

        if (i + 1) % 10 == 0:
            logger.info(f"Broadcasted {i + 1}/{len(state_dict)} parameters")

    logger.info(f"✓ Finished broadcasting {len(state_dict)} parameters")


def broadcast_shutdown_signal(
    src_rank: int = 0,
    device: Optional[torch.device] = None,
    group=None,
):
    """
    Send shutdown signal to inference nodes.

    Args:
        src_rank: Source rank for broadcast (default: 0)
        device: Device for tensor (default: cuda if available, else cpu)
        group: Process group (default: None = all processes)
    """
    if device is None:
        device = torch.device("cuda" if torch.cuda.is_available() else "cpu")

    logger.info("Broadcasting shutdown signal")

    # Create metadata with shutdown flag
    metadata = torch.zeros(128, dtype=torch.long, device=device)
    metadata[0] = -1  # Shutdown flag

    dist.broadcast(metadata, src=src_rank, group=group)

    logger.info("✓ Shutdown signal sent")
