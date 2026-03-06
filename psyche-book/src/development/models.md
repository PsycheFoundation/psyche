# Implementing models

This codebase includes a set of sample programs that let you design, implement, and test model architectures without spinning up the whole Psyche p2p training architecture.

We currently only implement Llama and Deepseek (see `shared/modeling/src/models/`), but PRs are very welcome to add more architectures and model types.

The `train` binary, documented below, is useful to test how your model trains using AdamW vs DisTrO.

## Quick start: test a new architecture

The fastest way to test a new model architecture end-to-end:

### 1. Get a config.json

Grab a `config.json` from any HuggingFace model repo that uses your target architecture. For example, to test Qwen3 MoE:

```bash
# download just the config
curl -L https://huggingface.co/Qwen/Qwen3-30B-A3B/resolve/main/config.json -o ~/qwen3-config.json
```

### 2. Create a tiny model

Use `make-tiny-init.py` to shrink the model to minimal dimensions. This creates a few-million-parameter version that trains in seconds, so you can iterate quickly on architecture code.

```bash
nix develop .#python --command python scripts/make-tiny-init.py \
    --config ~/qwen3-config.json \
    --save ~/test-model \
    --tokenizer Qwen/Qwen3-30B-A3B
```

This will:

- Shrink all dimensions (hidden size, layers, heads, etc.) to the smallest valid values
- Create init weights as safetensors
- Download the tokenizer

The script ensures the shrunken model is still valid for tensor parallelism (TP=8).

### 3. Write a training config

Create a TOML file pointing at your tiny model. Here's a minimal config using dummy data (no dataset needed):

```toml
[config]
total_steps = 100

[model.LLM]
architecture = "HfLlama"
data_type = "Pretraining"
max_seq_len = 256

[model.LLM.data_location]
Dummy = true

[model.LLM.checkpoint.Hub]
repo_id = "./test-model/"

[model.LLM.lr_schedule.Cosine]
base_lr = 4.0e-4
warmup_steps = 10
warmup_init_lr = 0.0
total_steps = 100
final_lr = 4.0e-5

[model.LLM.optimizer.AdamW]
betas = [0.9, 0.95]
weight_decay = 0.1
eps = 1e-8
clip_grad_norm = 1.0
```

> Use `Dummy` data for quick smoke tests — it generates random tokens, so the loss won't go down, but it exercises the full forward/backward pass. Switch to a real dataset when you need to verify actual training.

### 4. Train

```bash
nix run .#train -- config ./my-config.toml
```

You can also pass runtime options:

```bash
nix run .#train -- config ./my-config.toml \
    --micro-batch 4 \
    --device cuda:0
```

## Running without a config file

For quick tests, you can skip the TOML entirely and pass everything as CLI args. This only supports local datasets.

```bash
nix run .#train -- \
    --model ./test-model/ \
    --data-path ./data/fineweb-10bt/ \
    --total-batch 2 \
    --micro-batch 1 \
    --architecture HfLlama
```

## make-tiny-init.py reference

The `make-tiny-init.py` script creates minimized versions of any HuggingFace-compatible model for fast iteration.

### Inputs

You can pass either a raw HF `config.json` or a TorchTitan TOML:

```bash
# from HF config.json
python scripts/make-tiny-init.py --config /path/to/config.json --save ./tiny-model

# from torchtitan TOML (also rewrites the TOML to point at the tiny checkpoint)
python scripts/make-tiny-init.py --toml /path/to/train.toml --save ./tiny-run
```

### Options

| Flag              | Description                                                             |
| ----------------- | ----------------------------------------------------------------------- |
| `--config`        | Path to a HF `config.json`                                              |
| `--toml`          | Path to a TorchTitan job TOML (mutually exclusive with `--config`)      |
| `--save`          | Output directory (required)                                             |
| `--tokenizer`     | HF repo or local path to copy tokenizer from                            |
| `--preserve`      | Config fields to keep at original values (e.g. `--preserve vocab_size`) |
| `--no-checkpoint` | Only write configs, don't create init weights                           |
| `--dtype`         | Dtype for saved weights (default: `bfloat16`)                           |
| `--device`        | Device to initialize model on                                           |
| `--repo`          | Push the result to this HF repo                                         |
| `--private`       | Make the pushed HF repo private                                         |

### What it shrinks

The script applies these rules to make the model as small as possible:

| Field                     | Shrunk to        |
| ------------------------- | ---------------- |
| `num_attention_heads`     | 8                |
| `num_key_value_heads`     | 8                |
| `head_dim`                | 16               |
| `hidden_size`             | 128 (8 × 16)     |
| `intermediate_size`       | 256              |
| `num_hidden_layers`       | 2                |
| `max_position_embeddings` | 256              |
| `vocab_size`              | 256              |
| `num_experts`             | 8                |
| `num_experts_per_tok`     | min(original, 2) |

Head counts are kept divisible by 8 so tensor parallelism still works. Unknown fields are left untouched, so this works with any model architecture.

> **Tip:** use `--preserve vocab_size` if you want to train on real tokenized data — otherwise the tiny vocab won't match your tokenizer.

## Datasets

The `train` binary accepts several data sources when using a config file:

| Data location  | Description                                                             |
| -------------- | ----------------------------------------------------------------------- |
| `Dummy`        | Random tokens. No setup needed — useful for smoke-testing architectures |
| `Local`        | A directory of `.ds` files on disk                                      |
| `Http`         | Data served over HTTP (single URL, numbered files, or GCS bucket)       |
| `Preprocessed` | A HuggingFace dataset repo                                              |

When running without a config, only local datasets are supported.

For a Llama 2 model, a pre-tokenized dataset is available at [emozilla/fineweb-10bt-tokenized-datatrove-llama2](https://huggingface.co/datasets/emozilla/fineweb-10bt-tokenized-datatrove-llama2/tree/main).
Psyche only needs the `.ds` files, and will load any/all `.ds` files in the specified folder — you can download just one for smaller tests.

## Adding a new model type

The `train` binary currently assumes your model is a Llama 2/3 model, a Deepseek v2/v3 model, a HuggingFace Transformers-compatible model, or a TorchTitan-compatible model and instantiates it via `(LlamaForCausalLM|DeepseekForCausalLM)::from_pretrained` or `(PythonCausalLM::new|PythonDistributedCausalLM::new)`.

We currently only support causal language models — to implement a new one, you can create a file similar to `llama_for_causal_lm` and implement your model, ensuring you provide a trait impl for `CausalLM`.

There's also support for models written in Python. See the [Python](./python.md) docs for more information.

You might also need to modify the data provider, if your data is structured in some way.
Since you're implementing the forward pass yourself, you can serve and interpret data passed from the data provider however you need.
The data provider currently only supports reading fixed-size batches from input files, so data batches with different sizes will require some additional work.

> PRs welcome for any new kinds of dataset loading!
