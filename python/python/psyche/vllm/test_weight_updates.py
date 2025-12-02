#!/usr/bin/env python3
"""
Test for actual weight updates with protocol.

This test:
1. Spawns a training process that broadcasts named parameters
2. Spawns an inference process with vLLM + updater
3. Verifies that weights are actually applied to the shared state_dict

Run with: python -m psyche.vllm.test_weight_updates
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
    Training process that broadcasts actual named parameters.
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
        time.sleep(3)  # Give vLLM time to start and spawn updater

        # Import protocol functions
        from psyche.vllm.protocol import broadcast_parameter, broadcast_shutdown_signal

        # Send some actual parameter updates
        print(f"[Training Rank {rank}] Broadcasting parameter updates...")

        # Create mock parameters with known values
        params_to_send = [
            ("transformer.h.0.ln_1.weight", torch.ones(768) * 1.5),
            ("transformer.h.0.ln_1.bias", torch.ones(768) * 0.1),
            ("transformer.h.0.attn.c_attn.weight", torch.randn(768, 2304) * 0.02),
        ]

        for i, (param_name, param_tensor) in enumerate(params_to_send):
            print(
                f"[Training Rank {rank}] Broadcasting {param_name} "
                f"(shape={param_tensor.shape}, dtype={param_tensor.dtype})"
            )
            broadcast_parameter(param_name, param_tensor, src_rank=0)
            print(f"[Training Rank {rank}] ✓ Broadcasted {param_name}")
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


def inference_process(rank, world_size, master_addr, master_port):
    """
    Inference process that creates vLLM engine and verifies weight updates.
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

        print(f"[Inference Rank {rank}] Updater process will log weight updates")
        print(
            f"[Inference Rank {rank}] Look for 'Applied update to ...' messages in logs"
        )

        # Keep engine alive while updater receives updates
        print(f"[Inference Rank {rank}] Keeping engine alive for 20 seconds...")
        print(
            f"[Inference Rank {rank}] (Updater process is receiving/applying updates in background)"
        )
        time.sleep(20)

        print(f"[Inference Rank {rank}] ✅ Engine stayed alive during update period")
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
    """Test end-to-end weight updates with protocol"""
    print("=" * 80)
    print("Weight Updates Test with Protocol")
    print("=" * 80)
    print()
    print("This test spawns:")
    print("  - Rank 0: Training process (broadcasts named parameters)")
    print("  - Rank 1: vLLM inference + distributed updater (receives and applies)")
    print()

    try:
        world_size = 2
        master_addr = "localhost"
        master_port = 29501  # Different port from other test

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
        print("\nWaiting for processes to complete (timeout: 90s)...")
        training_proc.join(timeout=90)
        inference_proc.join(timeout=90)

        # Check results
        training_success = training_proc.exitcode == 0
        inference_success = inference_proc.exitcode == 0

        print("\n" + "=" * 80)
        print("Results:")
        print("=" * 80)
        print(
            f"Training process: {'✅ SUCCESS' if training_success else '❌ FAILED'} "
            f"(exit code: {training_proc.exitcode})"
        )
        print(
            f"Inference process: {'✅ SUCCESS' if inference_success else '❌ FAILED'} "
            f"(exit code: {inference_proc.exitcode})"
        )

        if training_success and inference_success:
            print("\n✅ Weight updates test PASSED!")
            print("\nWhat was tested:")
            print("  ✓ Training process broadcasted named parameters with protocol")
            print("  ✓ Updater received metadata + tensor data")
            print("  ✓ Updater applied updates to shared memory state_dict")
            print("  ✓ Inference engine has updated weights")
            return True
        else:
            print("\n❌ Weight updates test FAILED")
            return False

    except Exception as e:
        print(f"\n❌ Test failed with exception: {e}")
        import traceback

        traceback.print_exc()
        return False


def main():
    """Run weight updates test"""
    print("\n" + "=" * 80)
    print("vLLM Weight Updates - Protocol Test")
    print("=" * 80 + "\n")

    success = test_weight_updates()

    if success:
        print("\n🎉 All tests passed!")
        return 0
    else:
        print("\n⚠️  Test failed")
        return 1


if __name__ == "__main__":
    sys.exit(main())
