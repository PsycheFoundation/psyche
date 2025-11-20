"""
Weight Updater Daemon for vLLM

Runs in a separate process, receives weight updates via multiprocessing.Queue,
and applies them to vLLM's shared memory model.

This is inspired by TorchTitan's distributed_updater.py but adapted for Psyche's
architecture where weights come from DisTrO distributed training.
"""

import logging
import torch
import torch.multiprocessing as mp
from queue import Empty
from typing import Dict, Any, Optional, List
from dataclasses import dataclass

from .transforms import (
    apply_qkv_fusion,
    apply_gate_up_fusion,
    apply_rotary_permute,
)

logger = logging.getLogger(__name__)


@dataclass
class WeightUpdate:
    """Represents a single weight update operation"""

    param_name: str
    delta: torch.Tensor
    transform: Optional[str] = None
    transform_config: Optional[Dict[str, Any]] = None

    def __post_init__(self):
        if self.transform_config is None:
            self.transform_config = {}


class WeightUpdater:
    """
    Daemon process that applies weight updates to vLLM model.

    This runs in a separate process spawned from the main training process.
    It shares the vLLM model's memory (via .share_memory_()) and applies
    updates received through a multiprocessing queue.

    The updater handles:
    - Weight delta application (w = w + Δw)
    - Weight transformations (QKV fusion, gate-up fusion, rotary permutation)
    - Error recovery (revert to last known good state on failure)
    - Grouped updates (fusing Q, K, V projections together)
    """

    def __init__(
        self,
        model: torch.nn.Module,
        weight_queue: mp.Queue,
        transform_config: Dict[str, Dict[str, Any]],
        update_mode: str = "delta",
    ):
        """
        Args:
            model: The vLLM model (already in shared memory via share_memory_())
            weight_queue: Queue for receiving weight updates from training process
            transform_config: Parameter-specific transformation configs
            update_mode: "delta" (w += Δw) or "full" (w = w_new)
        """
        self.model = model
        self.weight_queue = weight_queue
        self.transform_config = transform_config
        self.update_mode = update_mode

        # Build parameter registry for fast lookups
        self.param_registry = {name: param for name, param in model.named_parameters()}

        # Checkpoint last known good state for error recovery
        self.last_good_state = {}
        self._checkpoint_state()

        # Buffers for grouped transformations (QKV, gate-up)
        self.qkv_buffers: Dict[str, Dict[str, torch.Tensor]] = {}
        self.gate_up_buffers: Dict[str, Dict[str, torch.Tensor]] = {}

        logger.info(
            f"WeightUpdater initialized with {len(self.param_registry)} parameters"
        )
        logger.info(f"Update mode: {update_mode}")
        logger.info(f"Transform config: {len(transform_config)} parameters")

    def _checkpoint_state(self):
        """Save current model state for error recovery"""
        with torch.no_grad():
            for name, param in self.param_registry.items():
                self.last_good_state[name] = param.data.clone()
        logger.debug("Checkpointed current model state")

    def _restore_last_good_state(self):
        """Restore model to last known good state"""
        with torch.no_grad():
            for name, param in self.param_registry.items():
                if name in self.last_good_state:
                    param.data.copy_(self.last_good_state[name])
        logger.warning("Restored model to last known good state")

    def run(self):
        """Main update loop (blocking)"""
        logger.info("WeightUpdater daemon started")

        while True:
            try:
                # Block for up to 1 second waiting for updates
                update = self.weight_queue.get(timeout=1.0)

                if update == "SHUTDOWN":
                    logger.info("Received shutdown signal")
                    break

                if update == "CHECKPOINT":
                    self._checkpoint_state()
                    continue

                if isinstance(update, dict):
                    self.apply_batch_update(update)
                elif isinstance(update, WeightUpdate):
                    self.apply_single_update(update)
                else:
                    logger.warning(f"Unknown update type: {type(update)}")

            except Empty:
                continue
            except Exception as e:
                logger.error(f"Error in update loop: {e}", exc_info=True)
                logger.warning("Attempting to restore last good state...")
                self._restore_last_good_state()

        logger.info("WeightUpdater daemon stopped")

    def apply_batch_update(self, update_dict: Dict[str, torch.Tensor]):
        """
        Apply a batch of weight updates.

        Args:
            update_dict: {param_name: delta_tensor}
        """
        try:
            with torch.no_grad():
                for param_name, delta_tensor in update_dict.items():
                    if param_name not in self.param_registry:
                        logger.warning(f"Parameter {param_name} not found in model")
                        continue

                    # Check if this parameter needs transformation
                    if param_name in self.transform_config:
                        self._apply_transformed_update(param_name, delta_tensor)
                    else:
                        self._apply_direct_update(param_name, delta_tensor)

            logger.debug(f"Applied batch update to {len(update_dict)} parameters")

            # Checkpoint after successful batch update
            self._checkpoint_state()

        except Exception as e:
            logger.error(f"Error applying batch update: {e}", exc_info=True)
            self._restore_last_good_state()
            raise

    def apply_single_update(self, update: WeightUpdate):
        """Apply a single weight update"""
        try:
            with torch.no_grad():
                if update.param_name not in self.param_registry:
                    logger.warning(f"Parameter {update.param_name} not found")
                    return

                # Apply transformation if specified
                delta = update.delta
                if update.transform:
                    delta = self._apply_transform_by_type(
                        update.transform, delta, update.transform_config
                    )

                # Update the parameter
                self._apply_direct_update(update.param_name, delta)

            logger.debug(f"Applied update to {update.param_name}")

        except Exception as e:
            logger.error(f"Error applying single update: {e}", exc_info=True)
            self._restore_last_good_state()
            raise

    def _apply_direct_update(self, param_name: str, delta_tensor: torch.Tensor):
        """Apply weight update directly without transformation"""
        param = self.param_registry[param_name]

        # Ensure delta is on same device and dtype
        delta_tensor = delta_tensor.to(param.device, param.dtype)

        if self.update_mode == "delta":
            param.data += delta_tensor
        else:  # "full"
            param.data.copy_(delta_tensor)

    def _apply_transformed_update(self, param_name: str, delta_tensor: torch.Tensor):
        """
        Apply weight update with transformation (QKV fusion, gate-up fusion, etc.)

        For grouped transformations (QKV, gate-up), we need to buffer components
        until all parts are received, then fuse and apply.
        """
        config = self.transform_config[param_name]
        transform_type = config.get("type")

        if transform_type == "qkv_fusion":
            self._handle_qkv_update(param_name, delta_tensor, config)
        elif transform_type == "gate_up_fusion":
            self._handle_gate_up_update(param_name, delta_tensor, config)
        elif transform_type == "rotary_permute":
            # Rotary permutation can be applied immediately
            transformed = apply_rotary_permute(delta_tensor, config.get("n_heads"))
            self._apply_direct_update(param_name, transformed)
        else:
            logger.warning(f"Unknown transform type: {transform_type}")
            self._apply_direct_update(param_name, delta_tensor)

    def _handle_qkv_update(
        self, param_name: str, delta_tensor: torch.Tensor, config: Dict[str, Any]
    ):
        """
        Handle QKV fusion update.

        Since Q, K, V are separate in training but fused in vLLM, we need to:
        1. Buffer each component (Q, K, V)
        2. When all three are received, fuse them
        3. Apply the fused update to vLLM's qkv_proj parameter
        """
        component = config.get("component")  # "q", "k", or "v"
        layer_key = self._extract_layer_key(param_name)

        # Initialize buffer for this layer if needed
        if layer_key not in self.qkv_buffers:
            self.qkv_buffers[layer_key] = {}

        # Store this component
        self.qkv_buffers[layer_key][component] = delta_tensor

        # Check if we have all three components
        if set(self.qkv_buffers[layer_key].keys()) == {"q", "k", "v"}:
            # All components received, fuse them
            q_delta = self.qkv_buffers[layer_key]["q"]
            k_delta = self.qkv_buffers[layer_key]["k"]
            v_delta = self.qkv_buffers[layer_key]["v"]

            fused_delta = apply_qkv_fusion(
                q_delta,
                k_delta,
                v_delta,
                n_heads=config.get("n_heads"),
                n_kv_heads=config.get("n_kv_heads"),
                apply_rotary=True,
            )

            # Apply to vLLM's fused parameter
            vllm_param_name = self._get_vllm_qkv_param_name(layer_key)
            self._apply_direct_update(vllm_param_name, fused_delta)

            # Clear buffer
            del self.qkv_buffers[layer_key]

            logger.debug(f"Applied fused QKV update for {layer_key}")
        else:
            logger.debug(
                f"Buffered {component} for {layer_key}, "
                f"waiting for {3 - len(self.qkv_buffers[layer_key])} more components"
            )

    def _handle_gate_up_update(
        self, param_name: str, delta_tensor: torch.Tensor, config: Dict[str, Any]
    ):
        """
        Handle gate-up fusion update.

        Similar to QKV, we buffer gate (w1) and up (w3) projections,
        then fuse and apply when both are received.
        """
        component = config.get("component")  # "gate" or "up"
        layer_key = self._extract_layer_key(param_name)

        # Initialize buffer for this layer if needed
        if layer_key not in self.gate_up_buffers:
            self.gate_up_buffers[layer_key] = {}

        # Store this component
        self.gate_up_buffers[layer_key][component] = delta_tensor

        # Check if we have both components
        if set(self.gate_up_buffers[layer_key].keys()) == {"gate", "up"}:
            # Both components received, fuse them
            gate_delta = self.gate_up_buffers[layer_key]["gate"]
            up_delta = self.gate_up_buffers[layer_key]["up"]

            fused_delta = apply_gate_up_fusion(gate_delta, up_delta)

            # Apply to vLLM's fused parameter
            vllm_param_name = self._get_vllm_gate_up_param_name(layer_key)
            self._apply_direct_update(vllm_param_name, fused_delta)

            # Clear buffer
            del self.gate_up_buffers[layer_key]

            logger.debug(f"Applied fused gate-up update for {layer_key}")
        else:
            logger.debug(
                f"Buffered {component} for {layer_key}, "
                f"waiting for {2 - len(self.gate_up_buffers[layer_key])} more components"
            )

    def _apply_transform_by_type(
        self, transform_type: str, tensor: torch.Tensor, config: Dict[str, Any]
    ) -> torch.Tensor:
        """Apply transformation by type"""
        if transform_type == "qkv_fusion":
            # This shouldn't be called directly for QKV (handled by _handle_qkv_update)
            logger.warning("QKV fusion called directly, should use buffered handler")
            return tensor
        elif transform_type == "gate_up_fusion":
            # This shouldn't be called directly for gate-up
            logger.warning(
                "Gate-up fusion called directly, should use buffered handler"
            )
            return tensor
        elif transform_type == "rotary_permute":
            return apply_rotary_permute(tensor, config.get("n_heads"))
        else:
            logger.warning(f"Unknown transform type: {transform_type}")
            return tensor

    def _extract_layer_key(self, param_name: str) -> str:
        """
        Extract layer identifier from parameter name.

        Example: "model.layers.0.self_attn.q_proj.weight" -> "model.layers.0"
        """
        parts = param_name.split(".")
        # Find "layers" and take up to and including the next part (layer index)
        try:
            layers_idx = parts.index("layers")
            return ".".join(parts[: layers_idx + 2])
        except ValueError:
            logger.warning(f"Could not extract layer key from {param_name}")
            return param_name

    def _get_vllm_qkv_param_name(self, layer_key: str) -> str:
        """
        Get vLLM's fused QKV parameter name from layer key.

        Training: model.layers.0.self_attn.{q,k,v}_proj.weight
        vLLM:     model.layers.0.self_attn.qkv_proj.weight
        """
        return f"{layer_key}.self_attn.qkv_proj.weight"

    def _get_vllm_gate_up_param_name(self, layer_key: str) -> str:
        """
        Get vLLM's fused gate-up parameter name from layer key.

        Training: model.layers.0.mlp.{gate,up}_proj.weight
        vLLM:     model.layers.0.mlp.gate_up_proj.weight
        """
        return f"{layer_key}.mlp.gate_up_proj.weight"


def spawn_updater_process(
    model: torch.nn.Module,
    weight_queue: mp.Queue,
    transform_config: Dict[str, Dict[str, Any]],
    update_mode: str = "delta",
) -> mp.Process:
    """
    Spawns the updater as a daemon process.

    IMPORTANT: The model must have .share_memory_() called on it BEFORE
    calling this function!

    Args:
        model: vLLM model (must have share_memory_() called!)
        weight_queue: Queue for sending updates
        transform_config: Transformation configs for each parameter
        update_mode: "delta" (w += Δw) or "full" (w = w_new)

    Returns:
        Process handle

    Example:
        >>> engine = UpdatableLLMEngine(model_name="meta-llama/Llama-2-7b-hf")
        >>> model = engine.get_model()
        >>> engine.share_memory()  # CRITICAL!
        >>> weight_queue = mp.Queue()
        >>> updater = spawn_updater_process(model, weight_queue, {})
        >>> # Send updates
        >>> weight_queue.put({"param_name": delta_tensor})
        >>> # Shutdown
        >>> weight_queue.put("SHUTDOWN")
        >>> updater.join()
    """
    updater = WeightUpdater(model, weight_queue, transform_config, update_mode)

    process = mp.Process(target=updater.run, daemon=True)
    process.start()

    logger.info(f"Spawned updater process with PID {process.pid}")

    return process
