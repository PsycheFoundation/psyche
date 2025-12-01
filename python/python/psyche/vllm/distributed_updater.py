"""
Psyche Distributed Weight Updater for vLLM

This module provides distributed weight synchronization between Psyche training
processes and vLLM inference engines using torch.distributed.

Inspired by torchtitan's GRPO implementation.
"""

import logging
import torch
import torch.distributed as dist
from typing import Dict, Optional, Any
from collections import defaultdict

logger = logging.getLogger(__name__)


def permute_for_rotary(weight: torch.Tensor, n_heads: int) -> torch.Tensor:
    """
    Permute weight tensor for sliced rotary embeddings.

    Args:
        weight: Weight tensor to permute (2D matrix)
        n_heads: Number of attention heads

    Returns:
        Permuted weight tensor
    """
    if weight.dim() == 2:
        dim1, dim2 = weight.shape
        return (
            weight.view(n_heads, dim1 // n_heads // 2, 2, dim2)
            .transpose(1, 2)
            .reshape(dim1, dim2)
        )
    elif weight.dim() == 1:
        # 1D bias vector
        dim1 = weight.shape[0]
        return (
            weight.view(n_heads, dim1 // n_heads // 2, 2).transpose(1, 2).reshape(dim1)
        )
    else:
        logger.warning(
            f"Unexpected weight dimension for rotary permutation: {weight.dim()}"
        )
        return weight


def weight_updater_process(
    state_dict: Dict[str, torch.Tensor],
    process_group_config: Dict[str, Any],
    model_config: Any,
):
    """
    Weight updater process that receives updates via torch.distributed.

    This process runs in a separate Python process and joins a torch.distributed
    process group. It receives weight updates and applies them to the shared
    memory state_dict.

    This is the inference side only - the training side will broadcast updates
    that this process receives and applies.

    Args:
        state_dict: Shared memory state_dict from vLLM model
        process_group_config: Configuration for torch.distributed
            - backend: "nccl" or "gloo"
            - init_method: e.g. "tcp://localhost:12345" or "env://"
            - world_size: Total number of processes in the group
            - rank: This process's rank in the group
        model_config: vLLM model configuration
    """
    backend = process_group_config.get("backend", "nccl")
    init_method = process_group_config.get("init_method")
    world_size = process_group_config.get("world_size")
    rank = process_group_config.get("rank")

    logger.info(
        f"Starting weight updater process: rank={rank}, world_size={world_size}, backend={backend}"
    )

    # Initialize process group
    dist.init_process_group(
        backend=backend,
        init_method=init_method,
        world_size=world_size,
        rank=rank,
    )

    logger.info(f"Process group initialized successfully (rank {rank}/{world_size})")

    # Get device from state_dict
    my_device = list(state_dict.values())[0].device
    logger.info(f"Using device: {my_device}")

    logger.info(
        f"Ready to receive weight updates. State dict has {len(state_dict)} parameters."
    )

    # Simple update loop - just wait for broadcasts from training
    # The training side will handle all the logic about what to send and when
    with torch.no_grad():
        logger.info("Entering update receive loop...")

        # For now, just keep the process alive and joined to the group
        # The actual update logic will be implemented when we connect the training side
        try:
            while True:
                # Wait for a signal/tensor from training
                # This is a placeholder - actual implementation will depend on
                # how Psyche's training side wants to send updates
                import time

                time.sleep(1)

        except KeyboardInterrupt:
            logger.info("Received interrupt, shutting down")

    logger.info("Weight updater process exiting")
    dist.destroy_process_group()
