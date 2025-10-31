# Python Integration

> [!WARNING]
> Python support is still under development and not production-ready.
> The APIs used to write it are not documented
> because they are still subject to large amounts of change.

## Overview

Psyche provides a Python integration that allows you to write modeling code in Python using libraries like [Hugging Face Transformers](https://github.com/huggingface/transformers) while leveraging Psyche's Rust core for training orchestration. This integration is designed for research where you want the flexibility of Python modeling with Psyche's training infrastructure, and production-scale training where you want to take advantage of highly optimized training frameworks already built in Python.

The Python integration works through a "sidecar" process that Psyche spawns and communicates with during training.

## Development Setup

To develop with the Python integration, we have a Python development shell available.

This shell provides:

- The `psyche` Python module (built from Rust using PyO3)
- PyTorch
- Transformers library
- Other required Python dependencies

### Development Workflow

When you enter the dev shell, it compiles the Rust extension that provides the `psyche` Python module. **If you modify any Rust code in the Python extension or its dependencies, you must exit and re-enter the dev shell** to recompile the extension.

We recommend running commands directly through the dev shell without entering it, which will recompile the extension as needed.

For example, to run the `train` program using python:

```bash
nix develop .#dev-python --command cargo run --features python --example train -- \
  --model emozilla/llama2-20m-init \
  --data-path ./data/fineweb-10bt/ \
  --total-batch 2 \
  --micro-batch 1 \
  --python
```

Alternatively, you can enter the shell with

```bash
nix develop .#dev-python
```

but this is likely to be a footgun as it's easy to forget to exit and re-enter the shell.

## Architecture

The Python integration uses a sidecar architecture:

1. **Psyche Core (Rust)**: Handles data loading, distributed training coordination, and spawns Python processes
2. **Python Sidecar**: Runs the modeling code using PyTorch and Transformers or any other Python code.

When you use the `--python` flag, Psyche automatically spawns Python sidecar processes using:

```bash
python -m psyche.sidecar --parent-pid <pid> --backend <backend> --init-method <method> --world-size <size> --rank <rank>
```

## Testing Your Changes

To test modifications to the Python integration:

1. **Modify the sidecar code** in the Python extension
2. **Run the training example**:

```bash
nix develop .#dev-python --command cargo run --features python --example train -- \
  --model emozilla/llama2-20m-init \
  --data-path ./data/fineweb-10bt/ \
  --total-batch 2 \
  --micro-batch 1 \
  --python
```

## How It Works

1. **Initialization**: Psyche spawns Python sidecar processes for each rank
2. **Model Creation**: The sidecar receives model architecture and source information via the distributed store
3. **Training Loop**: Psyche coordinates training by sending operations (train, optimize, extract) to the sidecar
4. **Data Flow**: Training data is broadcast to all processes, and gradients/parameters are communicated back through PyTorch's distributed primitives

The sidecar handles three main operations:

- **Train**: Forward/backward pass with gradient accumulation
- **Optimize**: Apply DisTrO results to the model being trained
- **Extract**: Model state extraction for checkpointing

This architecture allows you to write complex modeling code in Python while integrating with Psyche's distributed training network.
