import logging
from typing import List, Optional, Dict, Any
import torch

# Try to import vLLM, handle failure gracefully for non-inference nodes
try:
    from vllm import LLMEngine, EngineArgs, SamplingParams, RequestOutput
    from vllm.utils import Counter

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
        """
        for (
            name,
            param,
        ) in (
            self.engine.model_executor.driver_worker.model_runner.model.named_parameters()
        ):
            self.param_registry[name] = param

        logger.info(
            f"Registered {len(self.param_registry)} parameters for dynamic updates."
        )

    def add_request(self, prompt: str, sampling_params_dict: Dict[str, Any]) -> str:
        """
        Adds a request to the engine. Returns the request_id.
        """
        request_id = str(self.request_counter.next())

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

    def get_model(self) -> torch.nn.Module:
        """
        Returns the underlying model for weight updates.
        This is critical for the updater daemon to access parameters.

        Returns:
            The vLLM model (torch.nn.Module)
        """
        return self.engine.model_executor.driver_worker.model_runner.model

    def share_memory(self):
        """
        Makes model parameters accessible across processes via PyTorch shared memory.
        This enables zero-copy updates from the updater daemon.

        IMPORTANT: Must be called before spawning the updater process!
        """
        model = self.get_model()
        for param in model.parameters():
            param.share_memory_()
        logger.info("Model parameters moved to shared memory")

    def get_tokenizer(self):
        """Returns the tokenizer used by this engine."""
        return self.engine.tokenizer
