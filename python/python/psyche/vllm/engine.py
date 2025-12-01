import logging
from typing import List, Optional, Dict, Any
import torch

# Import and apply vLLM patches BEFORE importing vLLM
# This is critical for enabling shared memory access
try:
    from psyche.vllm.vllm_patch import (
        get_shared_state_dict,
        get_all_shared_state_dicts,
        get_state_dict_from_engine,
    )

    VLLM_PATCH_AVAILABLE = True
except ImportError:
    VLLM_PATCH_AVAILABLE = False


# Simple counter for request IDs
class Counter:
    def __init__(self, start: int = 0):
        self._counter = start

    def __next__(self):
        val = self._counter
        self._counter += 1
        return val


# Try to import vLLM, handle failure gracefully for non-inference nodes
try:
    from vllm import LLMEngine, EngineArgs, SamplingParams, RequestOutput

    VLLM_AVAILABLE = True
except ImportError:
    VLLM_AVAILABLE = False
    # Define dummy classes/types for type hinting if vLLM is missing
    LLMEngine = Any
    EngineArgs = Any
    SamplingParams = Any
    RequestOutput = Any

logger = logging.getLogger(__name__)


class UpdatableLLMEngine:
    """
    A wrapper around vLLM's LLMEngine that supports dynamic weight updates
    from a shared memory source (Psyche Distributed Updater).
    """

    def __init__(
        self,
        model_name: str,
        tensor_parallel_size: int = 1,
        dtype: str = "auto",
        max_model_len: Optional[int] = None,
        gpu_memory_utilization: float = 0.90,
    ):
        if not VLLM_AVAILABLE:
            raise ImportError("vLLM is not installed. Cannot start UpdatableLLMEngine.")

        logger.info(f"Initializing UpdatableLLMEngine with model: {model_name}")

        # Configure vLLM arguments
        engine_args = EngineArgs(
            model=model_name,
            tensor_parallel_size=tensor_parallel_size,
            dtype=dtype,
            max_model_len=max_model_len,
            gpu_memory_utilization=gpu_memory_utilization,
            enforce_eager=False,  # We might need eager mode if CUDA graphs break with dynamic weights
            disable_log_stats=False,
        )

        # Initialize the core engine
        self.engine = LLMEngine.from_engine_args(engine_args)
        self.request_counter = Counter()

        # Map parameter names to their PyTorch tensors for fast access during updates
        # We build this registry once at startup
        self.param_registry: Dict[str, torch.Tensor] = {}
        self._build_param_registry()

    def _build_param_registry(self):
        """
        Creates a mapping of parameter names to tensor objects.
        This allows O(1) access when applying updates.

        Tries multiple approaches in order:
        1. Shared memory state_dict (via patching) - most efficient
        2. Direct model_executor access (old vLLM versions)
        3. collective_rpc (vLLM 0.11+ without patching)
        """
        # For vLLM 0.11+ with patches, we use RPC-based in-place updates
        # We don't retrieve the state_dict because CUDA tensors can't be serialized
        if VLLM_PATCH_AVAILABLE:
            logger.info(
                "vLLM 0.11+ with Psyche patches detected. "
                "Weight updates will use RPC-based in-place modification."
            )
            # Store a flag that we're using patched mode
            self._using_patched_mode = True
            return

        # Check if we have the old-style direct model access
        if hasattr(self.engine, "model_executor"):
            try:
                for (
                    name,
                    param,
                ) in (
                    self.engine.model_executor.driver_worker.model_runner.model.named_parameters()
                ):
                    self.param_registry[name] = param

                logger.info(
                    f"Registered {len(self.param_registry)} parameters via model_executor."
                )
            except AttributeError as e:
                logger.warning(
                    f"Could not build param registry via model_executor: {e}. "
                    "Will use RPC-based weight updates instead."
                )
        else:
            logger.info(
                "vLLM 0.11+ detected without patches: using collective_rpc for weight updates."
            )

    def add_request(self, prompt: str, sampling_params_dict: Dict[str, Any]) -> str:
        """
        Adds a request to the engine. Returns the request_id.
        """
        request_id = str(next(self.request_counter))

        # Construct SamplingParams from dictionary
        # Default to simple greedy if not specified
        sampling_params = SamplingParams(**sampling_params_dict)

        self.engine.add_request(request_id, prompt, sampling_params)
        return request_id

    def step(self) -> List[RequestOutput]:
        """
        Performs one decoding step on the engine.
        This is meant to be called in a loop from Rust.
        """
        if self.engine.has_unfinished_requests():
            request_outputs = self.engine.step()
            return request_outputs
        return []

    def has_unfinished_requests(self) -> bool:
        return self.engine.has_unfinished_requests()

    def abort_request(self, request_id: str):
        self.engine.abort_request(request_id)

    def update_weights(self, weight_dict: Dict[str, torch.Tensor]):
        """
        Updates model weights using the appropriate method for the vLLM version.

        Tries methods in order of efficiency:
        1. RPC-based in-place updates (vLLM 0.11+ with patches) - works across processes
        2. Direct model_executor access (old vLLM versions)
        3. collective_rpc with load_weights (vLLM 0.11+ without patches)

        Args:
            weight_dict: Dictionary mapping parameter names to new weight tensors
        """
        # Method 1: RPC-based in-place updates (vLLM 0.11+ with patches)
        if hasattr(self, "_using_patched_mode") and self._using_patched_mode:
            logger.info(
                f"Updating {len(weight_dict)} weights via RPC in-place modification"
            )
            # Convert dict to list of tuples for serialization
            weight_updates = list(weight_dict.items())
            try:
                results = self.engine.collective_rpc(
                    "update_psyche_weights", args=(weight_updates,)
                )
                logger.info(f"Weight update completed on {len(results)} workers")
                return
            except Exception as e:
                logger.error(f"Failed to update weights via RPC: {e}")
                raise

        # Method 2: Direct shared memory access (if param_registry was populated)
        if self.param_registry and VLLM_PATCH_AVAILABLE:
            logger.debug(f"Updating {len(weight_dict)} weights via shared memory")
            updated_count = 0
            for name, new_weight in weight_dict.items():
                if name in self.param_registry:
                    self.param_registry[name].data.copy_(new_weight)
                    updated_count += 1
                else:
                    logger.warning(f"Parameter {name} not found in registry")
            logger.info(
                f"Updated {updated_count}/{len(weight_dict)} weights via shared memory"
            )
            return

        # Method 2: Old-style direct model_executor access
        if hasattr(self.engine, "model_executor") and self.param_registry:
            logger.info(f"Updating {len(weight_dict)} weights via model_executor")
            for name, new_weight in weight_dict.items():
                if name in self.param_registry:
                    self.param_registry[name].data.copy_(new_weight)
                else:
                    logger.warning(f"Parameter {name} not found in registry")
            return

        # Method 3: New vLLM 0.11+ approach using collective_rpc
        logger.info(f"Updating {len(weight_dict)} weights via collective_rpc")

        # Convert dict to list of (name, tensor) tuples for load_weights API
        weights = list(weight_dict.items())

        def apply_weights(model):
            """Function to apply weights via model.load_weights"""
            model.load_weights(weights=weights)
            return True

        try:
            results = self.engine.collective_rpc("apply_model", args=(apply_weights,))
            logger.info(f"Weight update completed on {len(results)} workers")
        except Exception as e:
            logger.error(f"Failed to update weights via collective_rpc: {e}")
            raise

    def get_model(self) -> Optional[torch.nn.Module]:
        """
        Returns the underlying model for weight updates.
        This is critical for the updater daemon to access parameters.

        Returns:
            The vLLM model (torch.nn.Module), or None if not accessible
            (e.g., in vLLM 0.11+ with V1 multiprocessing architecture)
        """
        if hasattr(self.engine, "model_executor"):
            try:
                return self.engine.model_executor.driver_worker.model_runner.model
            except AttributeError:
                logger.warning("Could not access model via model_executor")
                return None
        else:
            logger.warning(
                "Direct model access not available in vLLM 0.11+. "
                "Use update_weights() method with collective_rpc instead."
            )
            return None

    def share_memory(self):
        """
        Makes model parameters accessible across processes via PyTorch shared memory.
        This enables zero-copy updates from the updater daemon.

        IMPORTANT: Must be called before spawning the updater process!

        Note: In vLLM 0.11+, this may not be available or necessary due to
        the V1 multiprocessing architecture where the model runs in a separate process.
        """
        model = self.get_model()
        if model is not None:
            for param in model.parameters():
                param.share_memory_()
            logger.info("Model parameters moved to shared memory")
        else:
            logger.info(
                "Skipping share_memory: model not directly accessible in vLLM 0.11+"
            )

    def get_tokenizer(self):
        """Returns the tokenizer used by this engine."""
        return self.engine.tokenizer
