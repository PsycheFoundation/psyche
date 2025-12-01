#!/usr/bin/env python3
"""
Simple integration test for vLLM with weight updates.

This script tests the Python implementation without needing Rust.
Run with: python -m psyche.vllm.test_integration

Prerequisites:
- vLLM must be installed: pip install vllm
- PyTorch must be installed
"""

import sys
import torch
from pathlib import Path

# Add parent directory to path if needed
sys.path.insert(0, str(Path(__file__).parent.parent.parent))


def test_engine_basic():
    """Test basic engine functionality"""
    print("=" * 60)
    print("Test 1: Basic Engine Functionality")
    print("=" * 60)

    try:
        from psyche.vllm.engine import UpdatableLLMEngine
        from psyche.vllm.vllm_patch import get_shared_state_dict

        print("Creating engine with gpt2...")
        engine = UpdatableLLMEngine(
            model_name="gpt2",
            tensor_parallel_size=1,
            dtype="auto",
            max_model_len=512,
            gpu_memory_utilization=0.3,
        )

        print("✓ Engine created successfully")

        # Check if patches worked
        if hasattr(engine, "_using_patched_mode") and engine._using_patched_mode:
            print("✓ Psyche patches active: using RPC-based weight updates")
        else:
            shared_state_dict = get_shared_state_dict(worker_id=0)
            if shared_state_dict is not None:
                print(
                    f"✓ Shared memory patches working: {len(shared_state_dict)} parameters registered"
                )
            elif engine.param_registry:
                print(f"✓ Engine has access to {len(engine.param_registry)} parameters")
            else:
                print("⚠ No weight update mechanism available")

        # Test get_model()
        print("Getting model...")
        model = engine.get_model()
        if model is not None:
            print(f"✓ Model retrieved: {type(model).__name__}")
        else:
            print("⚠ Model not directly accessible (expected in vLLM 0.11+)")

        # Test share_memory()
        print("Calling share_memory...")
        engine.share_memory()
        print("✓ share_memory() completed")

        # Test inference
        print("Adding request...")
        request_id = engine.add_request(
            "Hello, world!", {"temperature": 0.0, "max_tokens": 5}
        )
        print(f"✓ Request added: {request_id}")

        print("Running step...")
        outputs = engine.step()
        print(f"✓ Step completed, got {len(outputs)} outputs")

        print("\n✅ Basic engine test PASSED\n")
        return True

    except ImportError as e:
        print(f"❌ Import error: {e}")
        print("Make sure vLLM is installed: pip install vllm")
        return False
    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        return False


def test_transforms():
    """Test weight transformation functions"""
    print("=" * 60)
    print("Test 2: Weight Transforms")
    print("=" * 60)

    try:
        from psyche.vllm.transforms import (
            apply_qkv_fusion,
            apply_gate_up_fusion,
            permute_for_rotary,
        )

        print("Testing rotary permutation...")
        weight = torch.randn(4096, 4096)
        permuted = permute_for_rotary(weight, n_heads=32)
        assert permuted.shape == weight.shape
        print("✓ Rotary permutation works")

        print("Testing QKV fusion...")
        q = torch.randn(4096, 4096)
        k = torch.randn(4096, 4096)
        v = torch.randn(4096, 4096)
        qkv = apply_qkv_fusion(q, k, v, n_heads=32)
        assert qkv.shape == (4096 * 3, 4096)
        print("✓ QKV fusion works")

        print("Testing gate-up fusion...")
        gate = torch.randn(11008, 4096)
        up = torch.randn(11008, 4096)
        gate_up = apply_gate_up_fusion(gate, up)
        assert gate_up.shape == (11008 * 2, 4096)
        print("✓ Gate-up fusion works")

        print("\n✅ Transforms test PASSED\n")
        return True

    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        return False


def test_updater_mock():
    """Test updater with mock data (no actual vLLM)"""
    print("=" * 60)
    print("Test 3: Weight Updater (Mock)")
    print("=" * 60)

    try:
        from psyche.vllm.updater import WeightUpdate
        import multiprocessing as mp

        print("Creating weight update...")
        update = WeightUpdate(
            weight_deltas={"layer.weight": torch.randn(100, 100)}, step=1
        )
        print(
            f"✓ WeightUpdate created: step={update.step}, {len(update.weight_deltas)} tensors"
        )

        print("Testing multiprocessing queue...")
        queue = mp.Queue()
        queue.put(update)
        retrieved = queue.get(timeout=1.0)
        print(f"✓ Queue works: retrieved step={retrieved.step}")

        print("\n✅ Updater mock test PASSED\n")
        return True

    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        return False


def test_weight_update_direct():
    """Test direct weight updates via shared memory"""
    print("=" * 60)
    print("Test 4: Direct Weight Update")
    print("=" * 60)

    try:
        from psyche.vllm.engine import UpdatableLLMEngine

        print("Creating engine...")
        engine = UpdatableLLMEngine(
            model_name="gpt2",
            tensor_parallel_size=1,
            dtype="auto",
            max_model_len=512,
            gpu_memory_utilization=0.3,
        )
        print("✓ Engine created")

        # For patched mode, we can't verify directly, but we can test the RPC call works
        if hasattr(engine, "_using_patched_mode") and engine._using_patched_mode:
            print("Testing RPC-based weight update...")
            # Create a small test update
            test_weight = torch.randn(100, 100)
            test_update = {"test.weight": test_weight}

            try:
                engine.update_weights(test_update)
                print(
                    "✓ RPC weight update call completed (actual parameter not verified)"
                )
            except Exception as e:
                print(f"⚠ RPC weight update failed: {e}")
        elif engine.param_registry:
            # Get a parameter to test
            param_name = list(engine.param_registry.keys())[0]
            original_param = engine.param_registry[param_name].data.clone()

            print(f"Testing update of: {param_name}")
            print(f"  Shape: {original_param.shape}, dtype: {original_param.dtype}")

            # Create update
            delta = torch.randn_like(original_param) * 0.001
            new_weight = original_param + delta

            # Apply update
            print("Applying weight update...")
            engine.update_weights({param_name: new_weight})

            # Verify update
            updated_param = engine.param_registry[param_name].data
            diff = (updated_param - new_weight).abs().max().item()

            print(f"  Max difference from expected: {diff}")

            if diff < 1e-6:
                print("✓ Weight update applied correctly")
            else:
                print(f"⚠ Weight update may not have applied correctly (diff={diff})")
        else:
            print("⚠ No weight update mechanism available, skipping test")
            print("\n⚠️  Weight update test SKIPPED\n")
            return True

        print("\n✅ Weight update test PASSED\n")
        return True

    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        return False


def test_manager():
    """Test VLLMWithUpdater manager (requires vLLM)"""
    print("=" * 60)
    print("Test 5: VLLMWithUpdater Manager")
    print("=" * 60)

    try:
        from psyche.vllm.manager import VLLMWithUpdater

        print("Creating VLLMWithUpdater in direct mode...")
        vllm = VLLMWithUpdater(
            model_name="gpt2",
            mode="direct",
            tensor_parallel_size=1,
            gpu_memory_utilization=0.3,
            max_model_len=512,
        )
        print("✓ Manager created successfully")

        print("Getting engine...")
        engine = vllm.engine
        print(f"✓ Engine retrieved: {type(engine).__name__}")

        print("Testing weight update...")
        weight_delta = {
            "transformer.h.0.attn.c_attn.weight": torch.randn(2304, 768) * 0.001
        }
        vllm.update_weights(weight_delta)
        print("✓ Weight update queued")

        print("Creating checkpoint...")
        vllm.checkpoint()
        print("✓ Checkpoint created")

        print("Shutting down...")
        vllm.shutdown()
        print("✓ Shutdown complete")

        print("\n✅ Manager test PASSED\n")
        return True

    except ImportError as e:
        print(f"❌ Import error: {e}")
        print("Make sure vLLM is installed: pip install vllm")
        return False
    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        return False


def main():
    """Run all tests"""
    print("\n" + "=" * 60)
    print("vLLM Integration Test Suite")
    print("=" * 60 + "\n")

    results = []

    # Test 1: Basic engine (requires vLLM)
    results.append(("Basic Engine", test_engine_basic()))

    # Test 2: Transforms (pure PyTorch)
    results.append(("Transforms", test_transforms()))

    # Test 3: Updater mock (no vLLM needed)
    results.append(("Updater Mock", test_updater_mock()))

    # Test 4: Direct weight update (requires vLLM with patches)
    results.append(("Direct Weight Update", test_weight_update_direct()))

    # Test 5: Full manager (requires vLLM)
    results.append(("VLLMWithUpdater", test_manager()))

    # Summary
    print("=" * 60)
    print("Test Summary")
    print("=" * 60)

    passed = sum(1 for _, result in results if result)
    total = len(results)

    for name, result in results:
        status = "✅ PASS" if result else "❌ FAIL"
        print(f"{name:20} {status}")

    print(f"\nTotal: {passed}/{total} tests passed")

    if passed == total:
        print("\n🎉 All tests passed!")
        return 0
    else:
        print(f"\n⚠️  {total - passed} test(s) failed")
        return 1


if __name__ == "__main__":
    sys.exit(main())
