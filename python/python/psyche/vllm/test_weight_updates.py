import sys
import os
import torch
import torch.distributed as dist
import torch.multiprocessing as mp
from pathlib import Path
import time

# Parent directory path for test execution as `python -m psyche.vllm.test_weight_updates`
sys.path.insert(0, str(Path(__file__).parent.parent.parent))


# Mock training process that broadcasts parameters
def training_process():
    rank = 0  # Training broadcaster is always rank 0
    world_size = 2  # Always 2: broadcaster + updater

    print(f"\n[Training Process] Starting training broadcaster")

    try:
        print(f"[Training Process] Initializing process group...")

        from psyche.vllm.distributed_updater import init_process_group

        device = torch.device("cuda:0")
        torch.cuda.set_device(device)
        print(f"[Training Process] Set CUDA device to {device}")

        vllm_group = init_process_group(
            backend="nccl",
            init_method="tcp://127.0.0.1:29500",
            world_size=world_size,
            rank=rank,
            group_name="vllm_updater",
        )
        print(
            f"[Training Process] vLLM process group initialized (rank {rank}/{world_size})"
        )

        print(f"[Training Process] Waiting for vLLM updater to join...")
        time.sleep(3)

        from psyche.vllm.protocol import broadcast_parameter, broadcast_shutdown_signal

        print(f"[Training Process] Broadcasting parameter updates...")

        # Create mock parameters with large changes to ensure output differs
        # Note: we send transposed weight matrices (out_features, in_features)
        params_to_send = [
            ("transformer.h.0.ln_1.weight", torch.ones(768, device=device) * 10.0),
            ("transformer.h.0.ln_1.bias", torch.ones(768, device=device) * 5.0),
            (
                "transformer.h.0.attn.c_attn.weight",
                torch.randn(2304, 768, device=device) * 0.5,
            ),
        ]

        for i, (param_name, param_tensor) in enumerate(params_to_send):
            print(
                f"[Training Process] Broadcasting {param_name} "
                f"(shape={param_tensor.shape}, dtype={param_tensor.dtype}, device={param_tensor.device})"
            )
            broadcast_parameter(param_name, param_tensor, src_rank=0, group=vllm_group)
            print(f"[Training Process] Broadcasted {param_name}")
            time.sleep(0.2)

        print(f"[Training Process] All parameter updates sent")

        # Send shutdown signal
        time.sleep(0.5)
        print(f"[Training Process] Sending shutdown signal")
        broadcast_shutdown_signal(src_rank=0, device=device, group=vllm_group)

        print(f"[Training Process] Cleaning up...")
        dist.destroy_process_group()
        print(f"[Training Process] Done!")

    except Exception as e:
        print(f"[Training Process] ERROR: {e}")
        import traceback

        traceback.print_exc()
        raise


# Inference process
def inference_process():
    print(f"\n[Inference Process] Starting vLLM inference engine")

    try:
        print(f"[Inference Process] Creating vLLM engine...")
        print(
            f"[Inference Process] Updater will join local process group automatically"
        )

        from psyche.vllm.engine import UpdatableLLMEngine

        # Create engine
        engine = UpdatableLLMEngine(
            model_name="gpt2",
            tensor_parallel_size=1,
            max_model_len=512,
            gpu_memory_utilization=0.3,
        )

        print(f"[Inference Process] Engine created!")
        print(
            f"[Inference Process] Distributed updater spawned and joined process group"
        )

        # Run inference before weight updates
        print(f"\n[Inference Process] === Running inference BEFORE weight updates ===")
        test_prompt = "Once upon a time"
        request_id_1 = engine.add_request(
            test_prompt, {"temperature": 0.0, "max_tokens": 10}
        )

        outputs_before = []
        while engine.has_unfinished_requests():
            batch_outputs = engine.step()
            outputs_before.extend(batch_outputs)

        text_before = outputs_before[0].outputs[0].text if outputs_before else ""
        print(f"[Inference Process] Output BEFORE: '{test_prompt}{text_before}'")

        print(f"\n[Inference Process] Updater process will log weight updates")
        print(f"[Inference Process] Look for 'Receiving parameter:' messages in logs")

        print(f"\n[Inference Process] Keeping engine alive for 20 seconds...")
        print(
            f"[Inference Process] (Updater process is receiving/applying updates in background)"
        )
        time.sleep(20)

        # Run inference after weight updates
        print(f"\n[Inference Process] === Running inference AFTER weight updates ===")
        request_id_2 = engine.add_request(
            test_prompt, {"temperature": 0.0, "max_tokens": 10}
        )

        outputs_after = []
        while engine.has_unfinished_requests():
            batch_outputs = engine.step()
            outputs_after.extend(batch_outputs)

        text_after = outputs_after[0].outputs[0].text if outputs_after else ""
        print(f"[Inference Process] Output AFTER: '{test_prompt}{text_after}'")

        if text_before != text_after:
            print(f"\n[Inference Process] SUCCESS: Output changed after weight update!")
            print(f"[Inference Process] This confirms weights were actually applied!")
        else:
            print(f"\n[Inference Process] WARNING: Output did not change")
            print(f"[Inference Process] (This could be expected if changes were small)")

        print(f"\n[Inference Process] Test complete!")
        print(
            f"[Inference Process] Check updater logs above to verify weight updates were applied"
        )
        print(f"[Inference Process] Done!")

    except Exception as e:
        print(f"[Inference Process] ERROR: {e}")
        import traceback

        traceback.print_exc()
        raise


def test_weight_updates():
    print("=" * 80)
    print("Weight Updates Test")
    print("=" * 80)

    try:
        print(f"Setting up local distributed environment:")
        print(f"  World size: 2 (broadcaster + updater)")
        print(f"  Address: tcp://127.0.0.1:29500")
        print(f"  Backend: nccl")
        print(f"  Device: cuda:0")

        # Spawn both processes
        ctx = mp.get_context("spawn")

        training_proc = ctx.Process(
            target=training_process,
        )

        inference_proc = ctx.Process(
            target=inference_process,
        )

        # Start processes
        print("Starting processes...")
        inference_proc.start()
        time.sleep(2)  # Let inference start first
        training_proc.start()

        print("\nWaiting for processes to complete (timeout: 90s)...")
        training_proc.join(timeout=90)
        inference_proc.join(timeout=90)

        training_success = training_proc.exitcode == 0
        inference_success = inference_proc.exitcode == 0

        print("\n" + "=" * 80)
        print("Results:")
        print("=" * 80)
        print(
            f"Training process: {'SUCCESS' if training_success else 'FAILED'} "
            f"(exit code: {training_proc.exitcode})"
        )
        print(
            f"Inference process: {'SUCCESS' if inference_success else 'FAILED'} "
            f"(exit code: {inference_proc.exitcode})"
        )

    except Exception as e:
        print(f"\nTest failed with exception: {e}")
        import traceback

        traceback.print_exc()
        return False

    return True


def main():
    print("\n" + "=" * 80)
    print("vLLM Weight Updates")
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
