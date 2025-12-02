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

    # Check environment variables needed for env:// init
    import os

    if init_method == "env://":
        master_addr = os.environ.get("MASTER_ADDR", "NOT_SET")
        master_port = os.environ.get("MASTER_PORT", "NOT_SET")
        logger.info(
            f"Using env:// init method - MASTER_ADDR={master_addr}, MASTER_PORT={master_port}"
        )

        if master_addr == "NOT_SET" or master_port == "NOT_SET":
            logger.error("MASTER_ADDR or MASTER_PORT not set in environment!")
            raise RuntimeError(
                "Cannot initialize process group: MASTER_ADDR/MASTER_PORT not set"
            )

    # Initialize process group
    logger.info(
        f"Calling dist.init_process_group(backend={backend}, init_method={init_method}, world_size={world_size}, rank={rank})"
    )
    try:
        dist.init_process_group(
            backend=backend,
            init_method=init_method,
            world_size=world_size,
            rank=rank,
        )
        logger.info(
            f"Process group initialized successfully (rank {rank}/{world_size})"
        )
    except Exception as e:
        logger.error(f"Failed to initialize process group: {e}")
        import traceback

        traceback.print_exc()
        raise

    # Get device from state_dict
    my_device = list(state_dict.values())[0].device
    logger.info(f"Using device: {my_device}")

    logger.info(
        f"Ready to receive weight updates. State dict has {len(state_dict)} parameters."
    )

    # Update receive loop - wait for broadcasts from training
    with torch.no_grad():
        logger.info("Entering update receive loop...")

        try:
            update_count = 0
            while True:
                # Receive broadcast from training process (rank 0)
                # For testing, we receive whatever tensor the training process sends
                # In production, this would be actual model parameters with metadata

                # Create a tensor to receive into
                # We don't know the size ahead of time, so we'll use a fixed size for testing
                received_tensor = torch.zeros(100, 100, device=my_device)

                logger.info(f"Waiting for broadcast #{update_count + 1}...")
                dist.broadcast(received_tensor, src=0)

                # Check if it's a shutdown signal (tensor with -1)
                if received_tensor[0, 0].item() == -1.0:
                    logger.info("Received shutdown signal, exiting")
                    break

                update_count += 1
                logger.info(
                    f"Received weight update #{update_count} (shape: {received_tensor.shape})"
                )

                # In production, we would apply this to state_dict:
                # state_dict[param_name].data.copy_(received_tensor)
                # For now, just log that we received it

        except KeyboardInterrupt:
            logger.info("Received interrupt, shutting down")
        except Exception as e:
            logger.error(f"Error in update receive loop: {e}")
            import traceback

            traceback.print_exc()

    logger.info(f"Weight updater process exiting (received {update_count} updates)")
    dist.destroy_process_group()
