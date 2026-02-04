import logging
from typing import Dict, Any, Optional

logger = logging.getLogger(__name__)

# Global registry of engines by ID
_engines: Dict[str, Any] = {}


def create_engine(
    engine_id: str,
    model_name: str,
    tensor_parallel_size: int = 1,
    dtype: str = "auto",
    max_model_len: Optional[int] = None,
    gpu_memory_utilization: float = 0.90,
) -> Dict[str, Any]:
    try:
        from psyche.vllm.engine import UpdatableLLMEngine

        logger.info(f"Creating engine '{engine_id}' with model '{model_name}'")

        engine = UpdatableLLMEngine(
            model_name=model_name,
            tensor_parallel_size=tensor_parallel_size,
            dtype=dtype,
            max_model_len=max_model_len,
            gpu_memory_utilization=gpu_memory_utilization,
        )

        _engines[engine_id] = engine

        logger.info(f"Engine '{engine_id}' created successfully")

        return {"status": "success", "engine_id": engine_id}

    except Exception as e:
        error_msg = f"Failed to create engine '{engine_id}': {e}"
        logger.error(error_msg)
        return {"status": "error", "error": error_msg}


def run_inference(
    engine_id: str,
    prompt: str,
    temperature: float = 1.0,
    top_p: float = 1.0,
    max_tokens: int = 100,
) -> Dict[str, Any]:
    try:
        if engine_id not in _engines:
            error_msg = f"Engine '{engine_id}' not found"
            logger.error(error_msg)
            return {"status": "error", "error": error_msg}

        engine = _engines[engine_id]

        sampling_params = {
            "temperature": temperature,
            "top_p": top_p,
            "max_tokens": max_tokens,
        }

        request_id = engine.add_request(prompt, sampling_params)

        # Process until complete
        outputs = []
        while engine.has_unfinished_requests():
            batch_outputs = engine.step()
            outputs.extend(batch_outputs)

        if outputs:
            output = outputs[0].outputs[0]
            return {
                "status": "success",
                "request_id": request_id,
                "generated_text": output.text,
                "prompt": prompt,
                "full_text": prompt + output.text,
            }
        else:
            return {
                "status": "error",
                "request_id": request_id,
                "error": "No output generated",
            }

    except Exception as e:
        error_msg = f"Inference failed for engine '{engine_id}': {e}"
        logger.error(error_msg)
        return {"status": "error", "request_id": request_id, "error": error_msg}


def shutdown_engine(engine_id: str) -> Dict[str, Any]:
    try:
        if engine_id not in _engines:
            error_msg = f"Engine '{engine_id}' not found"
            logger.error(error_msg)
            return {"status": "error", "error": error_msg}

        engine = _engines[engine_id]
        logger.info(f"Shutting down engine '{engine_id}'")

        engine.shutdown()
        del _engines[engine_id]

        logger.info(f"Engine '{engine_id}' shutdown complete")

        return {"status": "success", "engine_id": engine_id}

    except Exception as e:
        error_msg = f"Failed to shutdown engine '{engine_id}': {e}"
        logger.error(error_msg)
        return {"status": "error", "error": error_msg}


def get_engine_stats(engine_id: str) -> Dict[str, Any]:
    try:
        if engine_id not in _engines:
            error_msg = f"Engine '{engine_id}' not found"
            logger.error(error_msg)
            return {"status": "error", "error": error_msg}

        engine = _engines[engine_id]

        return {
            "status": "success",
            "engine_id": engine_id,
            "model_name": engine.model_name,
            "tensor_parallel_size": engine.tensor_parallel_size,
            "has_unfinished_requests": engine.has_unfinished_requests(),
        }

    except Exception as e:
        error_msg = f"Failed to get stats for engine '{engine_id}': {e}"
        logger.error(error_msg)
        return {"status": "error", "error": error_msg}


def list_engines() -> Dict[str, Any]:
    return {"status": "success", "engine_ids": list(_engines.keys())}
