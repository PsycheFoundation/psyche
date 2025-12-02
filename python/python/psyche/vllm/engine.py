import logging
from typing import List, Optional, Dict, Any
import torch

# Import and apply vLLM patches BEFORE importing vLLM
# This ensures GPUModelRunner is patched for distributed weight updates
try:
    from psyche.vllm import vllm_patch

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
    via torch.distributed (Psyche Distributed Updater).

    Weight updates are handled by the distributed updater process spawned
    by the patched GPUModelRunner, not through this wrapper's methods.
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

        logger.info(
            "UpdatableLLMEngine initialized. Weight updates will be handled by distributed updater."
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
        Weight updates are not supported via this method.

        Weight updates are handled by the distributed updater process that is spawned
        by the patched GPUModelRunner. Updates are sent via torch.distributed from
        training nodes to the inference process group.

        Args:
            weight_dict: Dictionary mapping parameter names to new weight tensors (ignored)
        """
        logger.warning(
            "update_weights() called, but weight updates are handled by the distributed updater process. "
            "Use torch.distributed to send updates to the training/inference process group."
        )

    def get_tokenizer(self):
        """Returns the tokenizer used by this engine."""
        return self.engine.tokenizer
