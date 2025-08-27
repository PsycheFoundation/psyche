# FSDP Batch Padding Documentation

## Problem Statement

When using Fully Sharded Data Parallel (FSDP) training with PyTorch and the `transformers` library, the batch size must be divisible by the world size (total number of GPUs). However, Psyche may provide batches of arbitrary sizes based on:

- Total number of nodes
- Global batch size configuration
- Data distribution across nodes
- Final batches in the dataset

This mismatch causes FSDP to fail with an error when the batch size is not divisible by the world size.

## Solution

The `PythonDistributedTrainer` now automatically pads batches with dummy samples when necessary to ensure divisibility by the world size. These padding samples:

- Have input_ids filled with padding tokens (0)
- Have labels set to `-100` (the ignore index in PyTorch loss functions)
- Do not contribute to the loss calculation
- Do not affect gradient computation

## Implementation Details

### Location

The padding logic is implemented in `/shared/modeling/src/python_distributed_trainer.rs`:

- Method: `pad_batch_for_fsdp()`
- Called in: `train()` method before GPU conversion

### How It Works

1. **Detection**: When a batch arrives, the trainer checks if its size is divisible by the world size
2. **Calculation**: If not divisible, calculates how many padding samples are needed
3. **Padding Creation**: Creates dummy samples with:
   - `input_ids`: Vector of zeros (padding token ID)
   - `labels`: Vector of -100 values (ignore index)
   - `position_ids`: Vector of zeros
   - `sequence_lengths`: Zeros or None (matching original batch format)
4. **Application**: Appends padding samples to the batch
5. **Verification**: Logs the padding operation for monitoring

### Example Scenarios

#### Scenario 1: Standard Training

- Command batch size: 128
- World size: 8
- Batch size per GPU: 16
- Result: No padding needed (128 % 8 = 0)

#### Scenario 2: Odd Batch Size

- Actual batch received: 127
- World size: 8
- Padding needed: 1 sample
- Final batch size: 128

#### Scenario 3: Small Final Batch

- Final batch size: 7
- World size: 8
- Padding needed: 1 sample
- Final batch size: 8

## Usage

No changes are required to use this feature. It's automatically enabled when:

1. Using `PythonDistributedTrainer`
2. Running with `--data-parallelism > 1`

### Command Example

```bash
cargo run --features parallelism,python --example train -- \
    --model NousResearch/Meta-Llama-3.1-8B \
    --data-path ./data/Hermes-3-Preprocessed-Llama3/data \
    --data-parallelism 8 \
    --micro-batch 1 \
    --sequence-length 4096 \
    --total-batch 128 \
    --learning-rate 0.000007 \
    --python
```

## Monitoring and Debugging

### Log Messages

The implementation provides several levels of logging:

1. **Info Level** (always shown):

   ```
   [FSDP Padding] Batch size 127 not divisible by world_size 8. Adding 1 padding samples.
   ```

2. **Debug Level** (with RUST_LOG=debug):

   ```
   FSDP world_size: 8
   Checking batch padding: original batch size = 127, world_size = 8
   Detailed padding info: batch_size=127, world_size=8, remainder=7, padding_needed=1
   Successfully padded batch: new size = 128 (divisible by 8)
   ```

3. **Trace Level** (with RUST_LOG=trace):
   ```
   Added padding sample 1 with seq_len=4096, labels=-100
   ```

### Verification

To verify padding is working correctly:

1. **Check logs**: Look for "FSDP Padding" messages
2. **Monitor loss**: Padding samples should not affect loss values
3. **Test with odd batch sizes**: Intentionally use batch sizes not divisible by world size

## Testing

### Unit Test

A test file is provided at `/shared/modeling/examples/test_fsdp_padding.rs` that verifies:

- Padding calculation correctness
- Padding sample format
- Original sample preservation
- Various batch size scenarios

### Integration Testing

To test with actual training:

1. Modify your batch size to be non-divisible (e.g., 127 instead of 128)
2. Run training with `--data-parallelism 8`
3. Verify training proceeds without FSDP errors
4. Check that loss values remain reasonable

## Troubleshooting

### Issue: FSDP still fails with size mismatch

- **Check**: Ensure you're using the latest `PythonDistributedTrainer`
- **Verify**: World size is correctly detected (check logs)
- **Confirm**: Batch is CPU format before padding

### Issue: Loss values seem incorrect

- **Check**: Padding labels are -100 (not 0 or other values)
- **Verify**: Your model's loss function respects ignore_index=-100
- **Monitor**: Number of padding samples (should be < world_size)

### Issue: Memory usage increased

- **Expected**: Padding adds samples, increasing memory usage
- **Mitigation**: The increase should be minimal (at most world_size-1 samples)
- **Alternative**: Adjust global batch size to be divisible by world_size

## Performance Considerations

1. **Memory Overhead**: Maximum additional samples = world_size - 1
2. **Computation**: Padded samples still go through forward pass (but not backward)
3. **Efficiency**: Consider adjusting batch sizes to minimize padding when possible

## Future Improvements

Potential enhancements to consider:

1. Configurable padding token ID (currently hardcoded to 0)
2. Option to drop samples instead of padding
3. Smart batching to minimize padding frequency
4. Metrics collection for padding statistics
