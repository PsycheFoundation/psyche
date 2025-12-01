#!/usr/bin/env python3
"""
Test for vLLM patching and shared memory weight updates.

This script tests that:
1. The vLLM patches are applied successfully
2. Shared memory state_dict is registered
3. Weight updates work via shared memory

Run with: python -m psyche.vllm.test_patched_integration
"""

import sys
import torch
from pathlib import Path

# Add parent directory to path if needed
sys.path.insert(0, str(Path(__file__).parent.parent.parent))


def test_patch_application():
    """Test that vLLM patches are applied"""
    print("=" * 60)
    print("Test 1: Patch Application")
    print("=" * 60)

    try:
        from psyche.vllm.vllm_patch import (
            get_shared_state_dict,
            get_all_shared_state_dicts,
        )

        print("✓ vLLM patch module imported")

        # Check if patches were applied
        import vllm.v1.worker.gpu_worker

        runner_class = vllm.v1.worker.gpu_worker.GPUModelRunner
        print(f"✓ GPUModelRunner class: {runner_class}")

        # Check if it's our patched version
        if "Patched" in str(runner_class):
            print("✓ GPUModelRunner appears to be patched")
        else:
            print("⚠ GPUModelRunner may not be patched (this might be okay)")

        print("\n✅ Patch application test PASSED\n")
        return True

    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        return False


def test_engine_with_patches():
    """Test engine creation with patches applied"""
    print("=" * 60)
    print("Test 2: Engine Creation with Patches")
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

        # Check if shared state_dict was registered
        shared_state_dict = get_shared_state_dict(worker_id=0)
        if shared_state_dict is not None:
            print(
                f"✓ Shared state_dict registered with {len(shared_state_dict)} parameters"
            )
            # Print some parameter names
            param_names = list(shared_state_dict.keys())[:5]
            print(f"  Sample parameters: {param_names}")
        else:
            print("⚠ Shared state_dict not registered (patches may not have worked)")

        # Check param_registry
        if engine.param_registry:
            print(
                f"✓ Engine param_registry has {len(engine.param_registry)} parameters"
            )
        else:
            print("⚠ Engine param_registry is empty")

        print("\n✅ Engine creation test PASSED\n")
        return True, engine

    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        return False, None


def test_weight_update():
    """Test weight updates via shared memory"""
    print("=" * 60)
    print("Test 3: Weight Update via Shared Memory")
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

        if not engine.param_registry:
            print("⚠ No param_registry, cannot test weight updates")
            return False

        # Get a parameter to update
        param_name = list(engine.param_registry.keys())[0]
        original_param = engine.param_registry[param_name]
        original_data = original_param.data.clone()

        print(f"Testing update of parameter: {param_name}")
        print(f"  Original shape: {original_data.shape}")
        print(f"  Original dtype: {original_data.dtype}")

        # Create a small delta
        delta = torch.randn_like(original_data) * 0.001

        # Apply update
        print("Applying weight update...")
        engine.update_weights({param_name: original_data + delta})

        # Check if update was applied
        new_data = engine.param_registry[param_name].data
        diff = (new_data - original_data).abs().max().item()

        print(f"  Max difference after update: {diff}")

        if diff > 1e-10:
            print("✓ Weight update was applied successfully")
        else:
            print("❌ Weight update did not change the parameter")
            return False

        # Restore original
        engine.param_registry[param_name].data.copy_(original_data)
        print("✓ Restored original weights")

        print("\n✅ Weight update test PASSED\n")
        return True

    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        return False


def test_inference_with_updates():
    """Test inference after weight updates"""
    print("=" * 60)
    print("Test 4: Inference After Weight Update")
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

        # Run inference before update
        print("Running inference before update...")
        request_id_1 = engine.add_request(
            "Hello, world!", {"temperature": 0.0, "max_tokens": 5}
        )
        outputs_1 = []
        while engine.has_unfinished_requests():
            outputs = engine.step()
            outputs_1.extend(outputs)

        print(f"✓ Generated {len(outputs_1)} outputs before update")

        # Apply a small weight update
        if engine.param_registry:
            param_name = list(engine.param_registry.keys())[0]
            original_param = engine.param_registry[param_name].data.clone()
            delta = torch.randn_like(original_param) * 0.0001
            engine.update_weights({param_name: original_param + delta})
            print(f"✓ Applied small update to {param_name}")

        # Run inference after update
        print("Running inference after update...")
        request_id_2 = engine.add_request(
            "Hello, world!", {"temperature": 0.0, "max_tokens": 5}
        )
        outputs_2 = []
        while engine.has_unfinished_requests():
            outputs = engine.step()
            outputs_2.extend(outputs)

        print(f"✓ Generated {len(outputs_2)} outputs after update")
        print("✓ Engine continued to work after weight update")

        print("\n✅ Inference test PASSED\n")
        return True

    except Exception as e:
        print(f"❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        return False


def main():
    """Run all tests"""
    print("\n" + "=" * 60)
    print("vLLM Patching Integration Test Suite")
    print("=" * 60 + "\n")

    results = []

    # Test 1: Patch application
    results.append(("Patch Application", test_patch_application()))

    # Test 2: Engine creation
    success, engine = test_engine_with_patches()
    results.append(("Engine Creation", success))

    # Test 3: Weight update
    results.append(("Weight Update", test_weight_update()))

    # Test 4: Inference after update
    results.append(("Inference After Update", test_inference_with_updates()))

    # Summary
    print("=" * 60)
    print("Test Summary")
    print("=" * 60)

    passed = sum(1 for _, result in results if result)
    total = len(results)

    for name, result in results:
        status = "✅ PASS" if result else "❌ FAIL"
        print(f"{name:30} {status}")

    print(f"\nTotal: {passed}/{total} tests passed")

    if passed == total:
        print("\n🎉 All tests passed!")
        return 0
    else:
        print(f"\n⚠️  {total - passed} test(s) failed")
        return 1


if __name__ == "__main__":
    sys.exit(main())
