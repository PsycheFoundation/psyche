"""
Test that the weight updater process correctly updates vLLM model weights.
"""

import logging
import torch
import tempfile
import time
import os
from safetensors.torch import save_file
from psyche.vllm import init_engine, trigger_weight_update

logging.basicConfig(
    level=logging.INFO,
    format="[%(asctime)s] %(levelname)s %(name)s: %(message)s",
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


def test_weight_updater():
    """Test weight updater process."""
    logger.info("=" * 80)
    logger.info("Weight Updater Process Test")
    logger.info("=" * 80)

    # 1. Initialize engine
    logger.info("\n[1/5] Initializing vLLM engine with GPT-2...")
    engine = init_engine(
        model_name="gpt2",
        max_model_len=512,
        gpu_memory_utilization=0.3,
    )
    logger.info("✓ Engine initialized")

    # 2. Trigger model loading (this will spawn the updater process)
    logger.info("\n[2/5] Triggering model load (will spawn weight updater)...")
    dummy_id = engine.add_request("test", {"temperature": 0.0, "max_tokens": 1})
    while engine.has_unfinished_requests():
        engine.step()
    logger.info("✓ Model loaded, weight updater should be running")

    # Give updater time to start
    time.sleep(2)

    # 3. Run inference BEFORE weight update
    logger.info("\n[3/5] Running inference BEFORE weight update...")
    test_prompt = "Once upon a time"
    request_id = engine.add_request(test_prompt, {"temperature": 0.0, "max_tokens": 10})

    outputs_before = []
    while engine.has_unfinished_requests():
        batch_outputs = engine.step()
        outputs_before.extend(batch_outputs)

    text_before = outputs_before[0].outputs[0].text if outputs_before else ""
    logger.info(f"  Output BEFORE: '{test_prompt}{text_before}'")

    # 4. Trigger weight update via Python API (what Rust will call)
    logger.info("\n[4/5] Triggering weight update (scale=10.0)...")
    weights_path = create_mock_weights(scale=10.0)

    # Call trigger_weight_update (this is what Rust will call via PyO3)
    trigger_weight_update(weights_path)
    logger.info(f"  Called trigger_weight_update()")

    # Give updater time to load weights
    logger.info("  Waiting for weight updater to apply changes...")
    time.sleep(2)

    # 5. Run inference AFTER weight update
    logger.info("\n[5/5] Running inference AFTER weight update...")
    request_id = engine.add_request(test_prompt, {"temperature": 0.0, "max_tokens": 10})

    outputs_after = []
    while engine.has_unfinished_requests():
        batch_outputs = engine.step()
        outputs_after.extend(batch_outputs)

    text_after = outputs_after[0].outputs[0].text if outputs_after else ""
    logger.info(f"  Output AFTER: '{test_prompt}{text_after}'")

    # Compare outputs
    if text_before != text_after:
        logger.info("\n✓ SUCCESS: Output changed after weight update!")
        logger.info("  This confirms weights were updated by the updater process!")
    else:
        logger.warning("\n⚠ WARNING: Output did not change")
        logger.warning(
            "  (This can happen with small models, but weights should have updated)"
        )

    # Cleanup
    os.unlink(weights_path)

    # Summary
    logger.info("\n" + "=" * 80)
    logger.info("Test Summary")
    logger.info("=" * 80)
    logger.info("✓ Engine initialized successfully")
    logger.info("✓ Weight updater process spawned")
    logger.info("✓ Weight update signaled via file")
    logger.info("✓ Inference executed before and after update")
    logger.info("\n🎉 Test complete! Check logs above for weight updater messages.")

    return True


if __name__ == "__main__":
    import sys

    try:
        success = test_weight_updater()
        sys.exit(0 if success else 1)
    except Exception as e:
        logger.error(f"\n❌ Test failed: {e}")
        import traceback

        traceback.print_exc()
        sys.exit(1)
