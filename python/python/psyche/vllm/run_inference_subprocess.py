import os
import json
import sys
from pathlib import Path
import multiprocessing

os.environ["VLLM_LOGGING_LEVEL"] = "ERROR"

multiprocessing.set_start_method("spawn", force=True)

sys.path.insert(0, str(Path(__file__).parent.parent.parent))


def run_inference(model_name: str, prompt: str) -> str:
    from psyche.vllm.engine import UpdatableLLMEngine

    original_stdout = sys.stdout
    original_stderr = sys.stderr

    generated_text = ""

    with open(os.devnull, "w") as devnull:
        sys.stdout = devnull
        sys.stderr = devnull

        try:
            engine = UpdatableLLMEngine(
                model_name=model_name,
                tensor_parallel_size=1,
                max_model_len=512,
                gpu_memory_utilization=0.3,
            )

            sampling_params = {
                "temperature": 0.0,  # Deterministic
                "max_tokens": 20,
            }

            request_id = engine.add_request(prompt, sampling_params)
            outputs = []
            while engine.has_unfinished_requests():
                outputs.extend(engine.step())

            if outputs[0] and outputs[0].outputs[0]:
                generated_text = outputs[0].outputs[0].text
        finally:
            sys.stdout = original_stdout
            sys.stderr = original_stderr
            engine.shutdown()

    return generated_text


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(
            "Usage: python run_inference_subprocess.py <model> <prompt>",
            file=sys.stderr,
        )
        sys.exit(1)

    model_name = sys.argv[1]
    prompt = sys.argv[2]

    try:
        generated = run_inference(model_name, prompt)
        result = {"generated_text": generated}
        print(json.dumps(result))
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        import traceback

        traceback.print_exc(file=sys.stderr)
        sys.exit(1)
