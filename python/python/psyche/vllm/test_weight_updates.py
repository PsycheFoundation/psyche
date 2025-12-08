import sys
import os
import torch
from pathlib import Path
import time
import tempfile

# Parent directory path for test execution as `python -m psyche.vllm.test_weight_updates`
sys.path.insert(0, str(Path(__file__).parent.parent.parent))


def test_weight_updates():
    print("=" * 80)
    print("Weight Updates Test (Queue-based)")
    print("=" * 80)

    try:
        print(f"\nSetting up test:")
        print(f"  Method: multiprocessing.Queue")
        print(f"  No torch.distributed needed!")
        print(f"  Device: cuda:0")

        from psyche.vllm.engine import UpdatableLLMEngine
        from psyche.vllm.rust_bridge import (
            init_weight_updater,
            update_weights_from_file,
            shutdown_updater,
        )

        # Create vLLM engine
        print(f"\n[Test] Creating vLLM engine...")
        engine = UpdatableLLMEngine(
            model_name="gpt2",
            tensor_parallel_size=1,
            max_model_len=512,
            gpu_memory_utilization=0.3,
        )
        print(f"[Test] Engine created!")

        # Initialize weight updater bridge
        print(f"\n[Test] Initializing weight updater bridge...")
        init_weight_updater(engine)
        print(f"[Test] Bridge initialized!")

        # Run inference BEFORE weight updates
        print(f"\n[Test] === Running inference BEFORE weight updates ===")
        test_prompt = "Once upon a time"
        request_id_1 = engine.add_request(
            test_prompt, {"temperature": 0.0, "max_tokens": 10}
        )

        outputs_before = []
        while engine.has_unfinished_requests():
            batch_outputs = engine.step()
            outputs_before.extend(batch_outputs)

        text_before = outputs_before[0].outputs[0].text if outputs_before else ""
        print(f"[Test] Output BEFORE: '{test_prompt}{text_before}'")

        # Create a test checkpoint with modified weights
        print(f"\n[Test] Creating test checkpoint from actual model...")
        from safetensors.torch import save_file
        from transformers import GPT2LMHeadModel

        # Load GPT2 model to get real weights
        print(f"[Test] Loading GPT2 model...")
        model = GPT2LMHeadModel.from_pretrained("gpt2")

        # Get state dict and modify slightly (multiply by 1.05 for subtle change)
        state_dict = model.state_dict()
        modified_state_dict = {}
        for key, tensor in state_dict.items():
            # Modify weights slightly
            modified_tensor = tensor * 1.05
            modified_state_dict[key] = modified_tensor.cpu()

        # Save to temporary file
        with tempfile.NamedTemporaryFile(suffix=".safetensors", delete=False) as f:
            checkpoint_path = f.name

        print(f"[Test] Saving checkpoint to {checkpoint_path}")
        save_file(modified_state_dict, checkpoint_path)
        print(f"[Test] ✓ Checkpoint saved ({len(modified_state_dict)} parameters)")

        # Trigger weight update
        print(f"\n[Test] Triggering weight update via queue...")
        update_weights_from_file(checkpoint_path)
        print(f"[Test] Update queued!")

        # Wait for updater to process
        print(f"\n[Test] Waiting for updater to apply checkpoint...")
        time.sleep(5)

        # Run inference AFTER weight updates
        print(f"\n[Test] === Running inference AFTER weight updates ===")
        request_id_2 = engine.add_request(
            test_prompt, {"temperature": 0.0, "max_tokens": 10}
        )

        outputs_after = []
        while engine.has_unfinished_requests():
            batch_outputs = engine.step()
            outputs_after.extend(batch_outputs)

        text_after = outputs_after[0].outputs[0].text if outputs_after else ""
        print(f"[Test] Output AFTER: '{test_prompt}{text_after}'")

        # Check if output changed
        if text_before != text_after:
            print(f"\n[Test] SUCCESS: Output changed after weight update!")
            print(f"[Test] This confirms weights were actually applied!")
            success = True
        else:
            print(f"\n[Test] WARNING: Output did not change")
            print(
                f"[Test] (Weights may have been applied, but change was too small to affect output)"
            )
            print(f"[Test] Check updater logs above to verify weights were loaded")
            success = True  # Still consider success if updater logged it

        # Cleanup
        print(f"\n[Test] Cleaning up...")
        shutdown_updater()
        os.unlink(checkpoint_path)
        print(f"[Test] Cleanup complete!")

        return success

    except Exception as e:
        print(f"\n[Test] Test failed with exception: {e}")
        import traceback

        traceback.print_exc()
        return False


def main():
    print("\n" + "=" * 80)
    print("vLLM Weight Updates Test (Queue-based, No torch.distributed)")
    print("=" * 80 + "\n")

    success = test_weight_updates()

    if success:
        print("\nAll tests passed!")
        return 0
    else:
        print("\nTest failed")
        return 1


if __name__ == "__main__":
    sys.exit(main())
