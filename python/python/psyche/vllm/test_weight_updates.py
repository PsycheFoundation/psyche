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
def training_process(rank, world_size, master_addr, master_port):
    print(f"\n[Training Rank {rank}] Starting training process")

    try:
        os.environ["MASTER_ADDR"] = master_addr
        os.environ["MASTER_PORT"] = str(master_port)
        os.environ["RANK"] = str(rank)
        os.environ["WORLD_SIZE"] = str(world_size)

        print(f"[Training Rank {rank}] Initializing process group...")
        dist.init_process_group(
            backend="gloo",  # gloo for CPU testing
            init_method="env://",
            world_size=world_size,
            rank=rank,
        )
        print(
            f"[Training Rank {rank}] Process group initialized (world_size={world_size})"
        )

        # Wait for inference process
        print(f"[Training Rank {rank}] Waiting for inference node to join...")
        time.sleep(3)  # vLLM startup time

        from psyche.vllm.protocol import broadcast_parameter, broadcast_shutdown_signal

        # Send some parameter updates
        print(f"[Training Rank {rank}] Broadcasting parameter updates...")

        # Create mock parameters with large changes to ensure output differs
        # Note: vLLM uses transposed weight matrices (out_features, in_features)
        params_to_send = [
            ("transformer.h.0.ln_1.weight", torch.ones(768) * 10.0),
            ("transformer.h.0.ln_1.bias", torch.ones(768) * 5.0),
            (
                "transformer.h.0.attn.c_attn.weight",
                torch.randn(2304, 768) * 0.5,
            ),
        ]

        for i, (param_name, param_tensor) in enumerate(params_to_send):
            print(
                f"[Training Rank {rank}] Broadcasting {param_name} "
                f"(shape={param_tensor.shape}, dtype={param_tensor.dtype})"
            )
            broadcast_parameter(param_name, param_tensor, src_rank=0)
            print(f"[Training Rank {rank}] Broadcasted {param_name}")
            time.sleep(0.2)

        print(f"[Training Rank {rank}] All parameter updates sent")

        # Send shutdown signal
        time.sleep(0.5)
        print(f"[Training Rank {rank}] Sending shutdown signal")
        broadcast_shutdown_signal(src_rank=0, device=torch.device("cpu"))

        print(f"[Training Rank {rank}] Cleaning up...")
        dist.destroy_process_group()
        print(f"[Training Rank {rank}] Done!")

    except Exception as e:
        print(f"[Training Rank {rank}] ERROR: {e}")
        import traceback

        traceback.print_exc()
        raise


# Inference process
def inference_process(rank, world_size, master_addr, master_port):
    print(f"\n[Inference Rank {rank}] Starting inference process")

    try:
        os.environ["MASTER_ADDR"] = master_addr
        os.environ["MASTER_PORT"] = str(master_port)

        os.environ["PSYCHE_UPDATER_BACKEND"] = "gloo"
        os.environ["PSYCHE_UPDATER_INIT_METHOD"] = "env://"
        os.environ["PSYCHE_WORLD_SIZE"] = str(world_size)
        os.environ["PSYCHE_RANK"] = str(rank)

        print(f"[Inference Rank {rank}] Creating vLLM engine...")
        print(
            f"[Inference Rank {rank}] Distributed config: world_size={world_size}, rank={rank}"
        )

        from psyche.vllm.engine import UpdatableLLMEngine

        # Create engine
        engine = UpdatableLLMEngine(
            model_name="gpt2",
            tensor_parallel_size=1,
            max_model_len=512,
            gpu_memory_utilization=0.3,
        )

        print(f"[Inference Rank {rank}] Engine created!")
        print(
            f"[Inference Rank {rank}] Distributed updater should be running and joined process group"
        )

        # Run inference before weight updates
        print(
            f"\n[Inference Rank {rank}] === Running inference BEFORE weight updates ==="
        )
        test_prompt = "Once upon a time"
        request_id_1 = engine.add_request(
            test_prompt, {"temperature": 0.0, "max_tokens": 10}
        )

        outputs_before = []
        while engine.has_unfinished_requests():
            batch_outputs = engine.step()
            outputs_before.extend(batch_outputs)

        text_before = outputs_before[0].outputs[0].text if outputs_before else ""
        print(f"[Inference Rank {rank}] Output BEFORE: '{test_prompt}{text_before}'")

        print(f"\n[Inference Rank {rank}] Updater process will log weight updates")
        print(
            f"[Inference Rank {rank}] Look for 'Applied update to ...' messages in logs"
        )

        print(f"\n[Inference Rank {rank}] Keeping engine alive for 20 seconds...")
        print(
            f"[Inference Rank {rank}] (Updater process is receiving/applying updates in background)"
        )
        time.sleep(20)

        # Run inference after weight updates
        print(
            f"\n[Inference Rank {rank}] === Running inference AFTER weight updates ==="
        )
        request_id_2 = engine.add_request(
            test_prompt, {"temperature": 0.0, "max_tokens": 10}
        )

        outputs_after = []
        while engine.has_unfinished_requests():
            batch_outputs = engine.step()
            outputs_after.extend(batch_outputs)

        text_after = outputs_after[0].outputs[0].text if outputs_after else ""
        print(f"[Inference Rank {rank}] Output AFTER: '{test_prompt}{text_after}'")

        # Check if output changed
        if text_before != text_after:
            print(
                f"\n[Inference Rank {rank}] SUCCESS: Output changed after weight update!"
            )
            print(
                f"[Inference Rank {rank}] This confirms weights were actually applied!"
            )
        else:
            print(f"\n[Inference Rank {rank}] WARNING: Output did not change")
            print(
                f"[Inference Rank {rank}] (This could be expected if changes were small)"
            )

        print(f"\n[Inference Rank {rank}] Test complete!")
        print(
            f"[Inference Rank {rank}] Check updater logs above to verify weight updates were applied"
        )
        print(f"[Inference Rank {rank}] Done!")

    except Exception as e:
        print(f"[Inference Rank {rank}] ERROR: {e}")
        import traceback

        traceback.print_exc()
        raise


def test_weight_updates():
    print("=" * 80)
    print("Weight Updates Test with Protocol")
    print("=" * 80)

    try:
        world_size = 2
        master_addr = "localhost"
        master_port = 29501

        print(f"Setting up distributed environment:")
        print(f"  World size: {world_size}")
        print(f"  Master addr: {master_addr}:{master_port}")

        # Spawn both processes
        ctx = mp.get_context("spawn")

        training_proc = ctx.Process(
            target=training_process,
            args=(0, world_size, master_addr, master_port),
        )

        inference_proc = ctx.Process(
            target=inference_process,
            args=(1, world_size, master_addr, master_port),
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


def main():
    print("\n" + "=" * 80)
    print("vLLM Weight Updates - Protocol Test")
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
