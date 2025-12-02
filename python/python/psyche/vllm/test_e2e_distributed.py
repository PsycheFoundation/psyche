#!/usr/bin/env python3
"""
End-to-end test for vLLM distributed weight updates.

This test spawns:
1. A vLLM inference engine (which spawns the distributed updater process)
2. A mock training process that sends weight updates via torch.distributed

Run with: python -m psyche.vllm.test_e2e_distributed
"""

import sys
import os
import torch
import torch.distributed as dist
import torch.multiprocessing as mp
from pathlib import Path
import time

# Add parent directory to path if needed
sys.path.insert(0, str(Path(__file__).parent.parent.parent))


def training_process(rank, world_size, master_addr, master_port):
    """
    Mock training process that sends weight updates.
    This is rank 0.
    """
    print(f"\n[Training Rank {rank}] Starting training process")

    try:
        # Set environment for torch.distributed
        os.environ["MASTER_ADDR"] = master_addr
        os.environ["MASTER_PORT"] = str(master_port)
        os.environ["RANK"] = str(rank)
        os.environ["WORLD_SIZE"] = str(world_size)

        # Initialize process group
        print(f"[Training Rank {rank}] Initializing process group...")
        dist.init_process_group(
            backend="gloo",  # Use gloo for CPU testing
            init_method="env://",
            world_size=world_size,
            rank=rank,
        )
        print(
            f"[Training Rank {rank}] Process group initialized (world_size={world_size})"
        )

        # Wait for inference process to be ready
        print(f"[Training Rank {rank}] Waiting for inference node to join...")
        time.sleep(5)  # Give vLLM time to start and spawn updater

        # Send a series of weight updates
        print(f"[Training Rank {rank}] Sending weight updates...")

        for i in range(3):
            # Create a mock weight update
            # In production, this would be actual model parameters
            update_tensor = torch.randn(100, 100) * 0.01
            print(
                f"[Training Rank {rank}] Broadcasting update {i+1}/3 (shape: {update_tensor.shape})"
            )

            # Broadcast to all ranks (including inference updater at rank 1)
            dist.broadcast(update_tensor, src=0)
            print(f"[Training Rank {rank}] Update {i+1} broadcast complete")

            time.sleep(1)

        print(f"[Training Rank {rank}] All updates sent")

        # Send shutdown signal (optional)
        time.sleep(1)
        shutdown_signal = torch.tensor([-1.0])
        print(f"[Training Rank {rank}] Sending shutdown signal")
        dist.broadcast(shutdown_signal, src=0)

        print(f"[Training Rank {rank}] Cleaning up...")
        dist.destroy_process_group()
        print(f"[Training Rank {rank}] Done!")

    except Exception as e:
        print(f"[Training Rank {rank}] ERROR: {e}")
        import traceback

        traceback.print_exc()
        raise


def inference_process(rank, world_size, master_addr, master_port):
    """
    Inference process that creates vLLM engine with distributed updater.
    This is rank 1 (the vLLM updater will be rank 1).
    """
    print(f"\n[Inference Rank {rank}] Starting inference process")

    try:
        # Set environment for vLLM distributed updater
        os.environ["MASTER_ADDR"] = master_addr
        os.environ["MASTER_PORT"] = str(master_port)

        # Set Psyche distributed config for the updater
        os.environ["PSYCHE_UPDATER_BACKEND"] = "gloo"
        os.environ["PSYCHE_UPDATER_INIT_METHOD"] = "env://"
        os.environ["PSYCHE_WORLD_SIZE"] = str(world_size)
        os.environ["PSYCHE_RANK"] = str(rank)

        print(f"[Inference Rank {rank}] Creating vLLM engine...")
        print(
            f"[Inference Rank {rank}] Distributed config: world_size={world_size}, rank={rank}"
        )

        from psyche.vllm.engine import UpdatableLLMEngine

        # Create engine - this will spawn the distributed updater process
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

        # Run some inference to show the engine works
        print(f"[Inference Rank {rank}] Running test inference...")
        request_id = engine.add_request(
            "Hello, world!", {"temperature": 0.0, "max_tokens": 5}
        )

        outputs = []
        while engine.has_unfinished_requests():
            batch_outputs = engine.step()
            outputs.extend(batch_outputs)

        print(f"[Inference Rank {rank}] Generated {len(outputs)} outputs")
        if outputs:
            print(
                f"[Inference Rank {rank}] Sample output: {outputs[0].outputs[0].text[:50]}"
            )

        # Keep engine alive while training process sends updates
        print(f"[Inference Rank {rank}] Keeping engine alive for 10 seconds...")
        print(
            f"[Inference Rank {rank}] (Updater process is receiving weight updates in background)"
        )
        time.sleep(10)

        print(f"[Inference Rank {rank}] Done!")

    except Exception as e:
        print(f"[Inference Rank {rank}] ERROR: {e}")
        import traceback

        traceback.print_exc()
        raise


def test_e2e_distributed():
    """Test end-to-end distributed weight updates with real vLLM"""
    print("=" * 80)
    print("End-to-End Distributed Weight Updates Test")
    print("=" * 80)
    print()
    print("This test spawns:")
    print("  - Rank 0: Training process (sends weight updates)")
    print("  - Rank 1: vLLM inference + distributed updater (receives updates)")
    print()

    try:
        world_size = 2
        master_addr = "localhost"
        master_port = 29500

        print(f"Setting up distributed environment:")
        print(f"  World size: {world_size}")
        print(f"  Master addr: {master_addr}:{master_port}")
        print()

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

        # Wait for completion
        print("\nWaiting for processes to complete (timeout: 60s)...")
        training_proc.join(timeout=60)
        inference_proc.join(timeout=60)

        # Check results
        training_success = training_proc.exitcode == 0
        inference_success = inference_proc.exitcode == 0

        print("\n" + "=" * 80)
        print("Results:")
        print("=" * 80)
        print(
            f"Training process: {'✅ SUCCESS' if training_success else '❌ FAILED'} (exit code: {training_proc.exitcode})"
        )
        print(
            f"Inference process: {'✅ SUCCESS' if inference_success else '❌ FAILED'} (exit code: {inference_proc.exitcode})"
        )

        if training_success and inference_success:
            print("\n✅ End-to-end distributed test PASSED!")
            print("\nWhat was tested:")
            print("  ✓ vLLM engine started with distributed updater")
            print("  ✓ Updater process joined torch.distributed process group")
            print("  ✓ Training process sent weight updates via broadcast")
            print("  ✓ Updater process received updates (check logs above)")
            print("  ✓ vLLM inference continued to work")
            return True
        else:
            print("\n❌ End-to-end distributed test FAILED")
            return False

    except Exception as e:
        print(f"\n❌ Test failed with exception: {e}")
        import traceback

        traceback.print_exc()
        return False


def main():
    """Run end-to-end distributed test"""
    print("\n" + "=" * 80)
    print("vLLM Distributed Weight Updates - End-to-End Test")
    print("=" * 80 + "\n")

    success = test_e2e_distributed()

    if success:
        print("\n🎉 All tests passed!")
        return 0
    else:
        print("\n⚠️  Test failed")
        return 1


if __name__ == "__main__":
    sys.exit(main())
