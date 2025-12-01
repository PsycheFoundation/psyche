"""
Psyche Weight Updater for vLLM

This module provides the PsycheWeightUpdater class which manages distributed
weight synchronization between Psyche training processes and vLLM inference engines.

Inspired by torchtitan's GRPO implementation but adapted for Psyche's architecture.
"""

import logging
import torch
import torch.distributed as dist
from typing import Dict, Optional, Callable, Any
from collections import defaultdict

logger = logging.getLogger(__name__)


class PsycheWeightUpdater:
    """
    Manages weight synchronization between Psyche training and vLLM inference.

    This class handles:
    1. Process group management for training and inference workers
    2. Receiving weight updates via torch.distributed
    3. Applying transformations (QKV fusion, gate-up fusion, etc.)
    4. Updating the vLLM model's shared memory state_dict
    """

    def __init__(
        self,
        state_dict: Dict[str, torch.Tensor],
        model_config: Any = None,
        training_world_size: int = 1,
        inference_world_size: int = 1,
        inference_rank: int = 0,
    ):
        """
        Initialize the weight updater.

        Args:
            state_dict: The vLLM model's shared memory state_dict
            model_config: vLLM model configuration for transformation metadata
            training_world_size: Number of training GPUs
            inference_world_size: Number of inference GPUs
            inference_rank: This inference worker's rank
        """
        self.state_dict = state_dict
        self.model_config = model_config
        self.training_world_size = training_world_size
        self.inference_world_size = inference_world_size
        self.inference_rank = inference_rank

        # Buffers for multi-tensor fusion operations
        self.qkv_buffer: Dict[str, torch.Tensor] = {}
        self.gate_up_buffer: Dict[str, torch.Tensor] = {}
        self.qkv_bias_buffer: Dict[str, torch.Tensor] = {}

        logger.info(
            f"PsycheWeightUpdater initialized: "
            f"training_world_size={training_world_size}, "
            f"inference_world_size={inference_world_size}, "
            f"inference_rank={inference_rank}"
        )

    def apply_weight_update(
        self,
        param_name: str,
        weight_tensor: torch.Tensor,
        needs_permute: bool = False,
        transform_type: Optional[str] = None,
    ):
        """
        Apply a single weight update to the state_dict.

        Args:
            param_name: Name of the parameter in the state_dict
            weight_tensor: New weight tensor to apply
            needs_permute: Whether this weight needs rotary permutation
            transform_type: Type of transformation ("qkv", "gate_up", etc.)
        """
        if param_name not in self.state_dict:
            logger.warning(f"Parameter {param_name} not found in state_dict")
            return

        target_tensor = self.state_dict[param_name]

        # Handle type conversion if needed
        if weight_tensor.dtype != target_tensor.dtype:
            weight_tensor = weight_tensor.to(target_tensor.dtype)

        # Apply transformations based on parameter type
        if transform_type == "qkv":
            self._handle_qkv_fusion(param_name, weight_tensor, needs_permute)
        elif transform_type == "gate_up":
            self._handle_gate_up_fusion(param_name, weight_tensor)
        elif needs_permute:
            weight_tensor = self._apply_permutation(weight_tensor)
            target_tensor.data.copy_(weight_tensor)
        else:
            # Direct copy
            target_tensor.data.copy_(weight_tensor)

    def _handle_qkv_fusion(
        self,
        param_name: str,
        weight_tensor: torch.Tensor,
        needs_permute: bool = False,
    ):
        """
        Handle QKV fusion for attention weights.

        vLLM expects Q, K, V weights to be concatenated along dim 0.
        We accumulate them and fuse when all three are received.
        """
        # Determine if this is Q, K, or V based on param name
        if ".wq." in param_name:
            key = "q"
        elif ".wk." in param_name:
            key = "k"
        elif ".wv." in param_name:
            key = "v"
        else:
            logger.warning(f"Cannot determine Q/K/V type for {param_name}")
            return

        # Apply permutation if needed
        if needs_permute and self.model_config is not None:
            n_heads = getattr(self.model_config, "num_attention_heads", 0)
            n_kv_heads = getattr(self.model_config, "num_key_value_heads", n_heads)
            if key == "q":
                weight_tensor = self._permute_for_rotary(weight_tensor, n_heads)
            elif key == "k":
                weight_tensor = self._permute_for_rotary(weight_tensor, n_kv_heads)

        # Store in buffer
        self.qkv_buffer[key] = weight_tensor

        # If we have all three, fuse and apply
        if len(self.qkv_buffer) == 3:
            fused = torch.cat(
                [self.qkv_buffer["q"], self.qkv_buffer["k"], self.qkv_buffer["v"]],
                dim=0,
            ).contiguous()

            # Find the fused parameter name (should contain "qkv_proj")
            fused_name = None
            for name in self.state_dict.keys():
                if "qkv_proj" in name:
                    fused_name = name
                    break

            if fused_name and fused_name in self.state_dict:
                self.state_dict[fused_name].data.copy_(fused)
                logger.debug(f"Applied fused QKV weights to {fused_name}")
            else:
                logger.warning("Could not find qkv_proj parameter for fusion")

            # Clear buffer
            self.qkv_buffer = {}

    def _handle_gate_up_fusion(self, param_name: str, weight_tensor: torch.Tensor):
        """
        Handle gate-up fusion for MLP weights.

        vLLM expects gate and up weights to be concatenated along dim 0.
        """
        # Determine if this is gate (w1) or up (w3)
        if ".w1." in param_name:
            key = "w1"
        elif ".w3." in param_name:
            key = "w3"
        else:
            logger.warning(f"Cannot determine gate/up type for {param_name}")
            return

        # Store in buffer
        self.gate_up_buffer[key] = weight_tensor

        # If we have both, fuse and apply
        if len(self.gate_up_buffer) == 2:
            fused = torch.cat(
                [self.gate_up_buffer["w1"], self.gate_up_buffer["w3"]], dim=0
            ).contiguous()

            # Find the fused parameter name
            fused_name = None
            for name in self.state_dict.keys():
                if "gate_up_proj" in name:
                    fused_name = name
                    break

            if fused_name and fused_name in self.state_dict:
                self.state_dict[fused_name].data.copy_(fused)
                logger.debug(f"Applied fused gate-up weights to {fused_name}")
            else:
                logger.warning("Could not find gate_up_proj parameter for fusion")

            # Clear buffer
            self.gate_up_buffer = {}

    def _permute_for_rotary(self, weight: torch.Tensor, n_heads: int) -> torch.Tensor:
        """
        Permute weight tensor for sliced rotary embeddings.

        Args:
            weight: Weight tensor to permute
            n_heads: Number of attention heads

        Returns:
            Permuted weight tensor
        """
        if weight.dim() == 2:
            # 2D weight matrix
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
                weight.view(n_heads, dim1 // n_heads // 2, 2)
                .transpose(1, 2)
                .reshape(dim1)
            )
        else:
            logger.warning(
                f"Unexpected weight dimension for rotary permutation: {weight.dim()}"
            )
            return weight

    def _apply_permutation(self, weight: torch.Tensor) -> torch.Tensor:
        """
        Apply generic permutation based on weight shape.

        This is a simplified version - you may need to adapt based on your model architecture.
        """
        if self.model_config is None:
            return weight

        n_heads = getattr(self.model_config, "num_attention_heads", 1)
        return self._permute_for_rotary(weight, n_heads)

    def update_from_dict(self, weight_dict: Dict[str, torch.Tensor]):
        """
        Update weights from a dictionary (simple case, no distributed comms).

        Args:
            weight_dict: Dictionary mapping parameter names to tensors
        """
        for name, tensor in weight_dict.items():
            self.apply_weight_update(name, tensor)

    def start_updater_loop(
        self,
        process_group: Optional[dist.ProcessGroup] = None,
        param_mappings: Optional[Dict[str, Dict[str, Any]]] = None,
    ):
        """
        Start the main updater loop for distributed weight synchronization.

        This would be run in a separate process/thread and listen for weight updates
        from the training processes via torch.distributed.

        Args:
            process_group: torch.distributed process group for communication
            param_mappings: Mapping of parameter names to metadata (for transformations)
        """
        logger.info("Starting PsycheWeightUpdater loop")

        if process_group is None:
            logger.warning("No process group provided, updater loop will not start")
            return

        # TODO: Implement the actual distributed update loop
        # This would be similar to torchtitan's weight_updater_process
        # but adapted for Psyche's communication patterns

        raise NotImplementedError("Distributed updater loop not yet implemented")
