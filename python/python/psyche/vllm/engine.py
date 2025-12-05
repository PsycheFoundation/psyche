import logging
import torch
from typing import List, Optional, Dict, Any


class Counter:
    def __init__(self, start: int = 0):
        self._counter = start

    def __next__(self):
        val = self._counter
        self._counter += 1
        return val


try:
    from vllm import LLMEngine, EngineArgs, SamplingParams, RequestOutput

    VLLM_AVAILABLE = True
except ImportError:
    VLLM_AVAILABLE = False
    LLMEngine = Any
    EngineArgs = Any
    SamplingParams = Any
    RequestOutput = Any

logger = logging.getLogger(__name__)


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
            raise ImportError("vLLM is not installed. Cannot start UpdatableLLMEngine.")

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
        self.request_counter = Counter()
        logger.info(f"Initialized vLLM engine with model: {model_name}")

    def load_weights(self, safetensors_path: str):
        from safetensors import safe_open
        import torch

        logger.info(f"Loading weights from: {safetensors_path}")

        state_dict = {}
        with safe_open(safetensors_path, framework="pt", device="cpu") as f:
            for key in f.keys():
                state_dict[key] = f.get_tensor(key)

        vllm_state_dict = self._get_vllm_state_dict()

        with torch.no_grad():
            loaded_count = 0
            for name, param in state_dict.items():
                if name in vllm_state_dict:
                    device = vllm_state_dict[name].device
                    dtype = vllm_state_dict[name].dtype
                    vllm_state_dict[name].data.copy_(
                        param.to(device=device, dtype=dtype)
                    )
                    loaded_count += 1
                else:
                    logger.warning(f"Parameter {name} not in vLLM model")

            logger.info(f"Loaded {loaded_count}/{len(state_dict)} parameters")

    def _get_vllm_state_dict(self) -> Dict[str, torch.Tensor]:
        from psyche.vllm.vllm_patch import get_shared_state_dict

        state_dict = get_shared_state_dict()
        if state_dict is None:
            raise RuntimeError(
                "Could not access vLLM state_dict. Make sure the model is loaded first."
            )
        return state_dict

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
