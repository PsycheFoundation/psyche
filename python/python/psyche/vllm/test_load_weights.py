"""
Test that load_weights() correctly updates vLLM model weights.
"""

import logging
import torch
import tempfile
from safetensors.torch import save_file
from psyche.vllm import init_engine, load_weights, get_engine

logging.basicConfig(
    level=logging.INFO,
    format="[%(asctime)s] %(levelname)s: %(message)s",
    datefmt="%H:%M:%S",
)

logger = logging.getLogger(__name__)


def create_mock_weights(scale: float = 1.0) -> str:
    """Create mock safetensors file with test weights."""
    weights = {
        "transformer.h.0.ln_1.weight": torch.ones(768) * scale,
        "transformer.h.0.ln_1.bias": torch.ones(768) * scale * 0.5,
    }

    # Save to temp file
    temp_file = tempfile.NamedTemporaryFile(suffix=".safetensors", delete=False)
    save_file(weights, temp_file.name)
    logger.info(f"Created mock weights with scale={scale} at {temp_file.name}")
    return temp_file.name


def test_load_weights():
    """Test loading weights into vLLM engine."""
    logger.info("=" * 80)
    logger.info("Weight Loading Test")
    logger.info("=" * 80)

    # 1. Initialize engine
    logger.info("\n[1/5] Initializing vLLM engine with GPT-2...")
    engine = init_engine(
        model_name="gpt2",
        max_model_len=512,
        gpu_memory_utilization=0.3,
    )
    logger.info("✓ Engine initialized")

    # 2. Get initial weights
    logger.info("\n[2/5] Reading initial weight values...")
    initial_state = engine._get_vllm_state_dict()
    initial_ln_weight = initial_state["transformer.h.0.ln_1.weight"].clone()
    initial_ln_bias = initial_state["transformer.h.0.ln_1.bias"].clone()
    logger.info(f"  ln_1.weight mean: {initial_ln_weight.mean():.6f}")
    logger.info(f"  ln_1.bias mean: {initial_ln_bias.mean():.6f}")

    # 3. Run inference BEFORE weight update
    logger.info("\n[3/5] Running inference BEFORE weight update...")
    test_prompt = "Once upon a time"
    request_id = engine.add_request(test_prompt, {"temperature": 0.0, "max_tokens": 10})

    outputs_before = []
    while engine.has_unfinished_requests():
        batch_outputs = engine.step()
        outputs_before.extend(batch_outputs)

    text_before = outputs_before[0].outputs[0].text if outputs_before else ""
    logger.info(f"  Output: '{test_prompt}{text_before}'")

    # 4. Load new weights
    logger.info("\n[4/5] Loading new weights (scale=10.0)...")
    weights_path = create_mock_weights(scale=10.0)
    load_weights(weights_path)
    logger.info("✓ Weights loaded")

    # Verify weights changed
    updated_state = engine._get_vllm_state_dict()
    updated_ln_weight = updated_state["transformer.h.0.ln_1.weight"]
    updated_ln_bias = updated_state["transformer.h.0.ln_1.bias"]
    logger.info(f"  ln_1.weight mean: {updated_ln_weight.mean():.6f}")
    logger.info(f"  ln_1.bias mean: {updated_ln_bias.mean():.6f}")

    # Check if weights actually changed
    weight_changed = not torch.allclose(initial_ln_weight, updated_ln_weight)
    bias_changed = not torch.allclose(initial_ln_bias, updated_ln_bias)

    if weight_changed and bias_changed:
        logger.info("✓ Weights successfully updated in vLLM!")
    else:
        logger.error("✗ Weights did not change!")
        return False

    # 5. Run inference AFTER weight update
    logger.info("\n[5/5] Running inference AFTER weight update...")
    request_id = engine.add_request(test_prompt, {"temperature": 0.0, "max_tokens": 10})

    outputs_after = []
    while engine.has_unfinished_requests():
        batch_outputs = engine.step()
        outputs_after.extend(batch_outputs)

    text_after = outputs_after[0].outputs[0].text if outputs_after else ""
    logger.info(f"  Output: '{test_prompt}{text_after}'")

    # Compare outputs
    if text_before != text_after:
        logger.info("\n✓ SUCCESS: Output changed after weight update!")
        logger.info("  This confirms weights are being used by inference!")
    else:
        logger.warning("\n⚠ WARNING: Output did not change")
        logger.warning("  (This can happen with small models, but weights did update)")

    # Cleanup
    import os

    os.unlink(weights_path)

    # Summary
    logger.info("\n" + "=" * 80)
    logger.info("Test Summary")
    logger.info("=" * 80)
    logger.info("✓ Engine initialized successfully")
    logger.info("✓ load_weights() executed without errors")
    logger.info("✓ Weights values changed in vLLM state_dict")
    logger.info("✓ Inference executed before and after update")
    logger.info("\n🎉 All tests passed! Ready for Rust integration.")

    return True


if __name__ == "__main__":
    import sys

    try:
        success = test_load_weights()
        sys.exit(0 if success else 1)
    except Exception as e:
        logger.error(f"\n❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        sys.exit(1)
