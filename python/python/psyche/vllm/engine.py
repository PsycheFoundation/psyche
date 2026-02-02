import logging
import ray
import time
from itertools import count
from typing import List, Optional, Dict, Any

logger = logging.getLogger(__name__)

try:
    from vllm import LLMEngine, EngineArgs, SamplingParams, RequestOutput

    VLLM_AVAILABLE = True
except ImportError:
    VLLM_AVAILABLE = False
    logger.warning("vLLM is not installed. UpdatableLLMEngine will not be available.")
    LLMEngine = Any
    EngineArgs = Any
    SamplingParams = Any
    RequestOutput = Any


# A wrapper around vLLM's LLMEngine that supports dynamic weight updates.
class UpdatableLLMEngine:
    def __init__(
        self,
        model_name: str,
        tensor_parallel_size: int = 1,
        dtype: str = "auto",
        max_model_len: Optional[int] = None,
        gpu_memory_utilization: float = 0.90,
    ):
        if not VLLM_AVAILABLE:
            raise ImportError(
                "vLLM is not installed. Please install it with: pip install vllm"
            )

        self.model_name = model_name
        self.tensor_parallel_size = tensor_parallel_size
        self.dtype = dtype
        self.max_model_len = max_model_len
        self.gpu_memory_utilization = gpu_memory_utilization

        logger.info(f"Initializing vLLM engine with model: {model_name}")

        engine_args = EngineArgs(
            model=model_name,
            tensor_parallel_size=tensor_parallel_size,
            dtype=dtype,
            max_model_len=max_model_len,
            gpu_memory_utilization=gpu_memory_utilization,
            enforce_eager=False,
            disable_log_stats=False,
        )

        self.engine = LLMEngine.from_engine_args(engine_args)
        self.request_counter = count()

        logger.info("vLLM engine initialized successfully")

    def add_request(self, prompt: str, sampling_params_dict: Dict[str, Any]) -> str:
        request_id = str(next(self.request_counter))

        sampling_params = SamplingParams(**sampling_params_dict)

        self.engine.add_request(request_id, prompt, sampling_params)
        return request_id

    def step(self) -> List[RequestOutput]:
        if self.engine.has_unfinished_requests():
            request_outputs = self.engine.step()
            return request_outputs
        return []

    def has_unfinished_requests(self) -> bool:
        return self.engine.has_unfinished_requests()

    def abort_request(self, request_id: str):
        self.engine.abort_request(request_id)

    def get_tokenizer(self):
        return self.engine.tokenizer

    def shutdown(self):
        logger.info("Shutting down vLLM engine")
        # vLLM handles cleanup automatically when the engine is destroyed
        if self.engine is not None:
            del self.engine

        try:
            if ray.is_initialized():
                logger.info("Shutting down Ray...")
                ray.shutdown()
                logger.info("Ray shutdown complete")
        except Exception as e:
            logger.warning(f"Failed to shutdown Ray: {e}")

        # Give time to clean up
        time.sleep(1)
        logger.info("vLLM engine shutdown complete")
