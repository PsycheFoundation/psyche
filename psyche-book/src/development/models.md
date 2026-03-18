# Implementing models

This codebase includes a set of sample programs that let you design, implement, and test model architectures without spinning up the whole Psyche p2p training architecture.

We currently only implement Llama and Deepseek (see `shared/modeling/src/models/`), but PRs are very welcome to add more architectures and model types.

The `train` binary, documented below, is useful to test how your model trains using AdamW vs DisTrO.

## Quickstart: test a new architecture

The fastest way to test a new model architecture end-to-end:

### 1. Create a tiny model

Use `make-tiny-init.py` to shrink any model to minimal dimensions. This creates a minimally sized (probably a few million parameters) version that loads & trains in seconds, so you can iterate quickly on architecture code.

You can pass an HF repo slug directly to have the script download the model config & tokenizer for you.

```bash
nix develop .#python --command python scripts/make-tiny-init.py \
    --repo Qwen/Qwen3-30B-A3B \
    --save ~/test-model
```

This will output a folder `~/test-model/checkpoint/`, with weights, config, and the tokenizer, and a file `~/test-model/train.toml`, a Psyche [run config](../enduser/run-config.md) with some default settings.

This script shrinks all dimensions of the model to the smallest possible valid values, while keeping the model able to be trained with tensor parallelism = 8. More information is available at [make-tiny-init.py reference](./make-tiny-init.md).

### 2. Train

The generated run config uses dummy data and sensible defaults to test.

```bash
cd ~/test-model
nix run .#train -- config ./train.toml
```

You can also pass runtime options:

```bash
nix run .#train -- config ./train.toml \
    --micro-batch 4 \
    --device cuda:0
```

> Since we're using `Dummy` data , we generate random tokens, so the loss won't go down, but we still run through the full forward/backward pass. Switch to a real dataset when you need to verify that training does make loss go down.

You can edit `train.toml` directly or write your own from scratch. See the [run configuration](../enduser/run-config.md) section for more info.

## Running without a config file

For quick tests, you can skip the TOML file entirely and pass everything as command line args.

```bash
nix run .#train -- \
    --model ./test-model/ \
    --data-path ./data/fineweb-10bt/ \
    --total-batch 2 \
    --micro-batch 1 \
    --architecture HfLlama
```

## Dumping the config from a live run

If there's a run on-chain and you want to reproduce its configuration locally (e.g. to create a tiny model for debugging, or to start a new run with the same settings), use `dump-config`:

```bash
nix run .#run-manager dump-config \
    --rpc https://api.devnet.solana.com \
    --run-id <your_run_id>
```

This will print the full run config to stdout. You can redirect it to a file and use it directly, or run it through make-tiny-init.py to make a smaller version for developing with.

```bash
# dump the run's config
nix run .#run-manager dump-config --rpc https://api.devnet.solana.com --run-id my-run > live-config.toml

# create a tiny version of that run's model for local testing
nix develop .#python --command python scripts/make-tiny-init.py \
    --psyche live-config.toml \
    --save /tmp/tiny-my-run
```

This is useful when you want to iterate on an architecture locally using the exact same model architecture and hypers as a production run.

## Datasets

The `train` binary accepts any psyche data source when using a run config..

| Data location  | Description                                                       |
| -------------- | ----------------------------------------------------------------- |
| `Dummy`        | Random tokens. (loss will not go down)                            |
| `Local`        | A directory of `.ds` files on disk                                |
| `Http`         | Data served over HTTP (single URL, numbered files, or GCS bucket) |
| `Preprocessed` | A HuggingFace dataset repo                                        |

When running without a config, only local datasets are supported.

For a Llama 2 model, a pre-tokenized dataset is available at [emozilla/fineweb-10bt-tokenized-datatrove-llama2](https://huggingface.co/datasets/emozilla/fineweb-10bt-tokenized-datatrove-llama2/tree/main).
Psyche only needs the `.ds` files, and will load any/all `.ds` files in the specified folder — you can download just one for smaller tests.

## Adding a new model type

Both the main Psyche client and the `train` binary support a handful of model types:
Llama 2/3 models, a Deepseek v2/v3 models, any HuggingFace Transformers-compatible model, or any TorchTitan-compatible model.

They're instantiated via `(LlamaForCausalLM|DeepseekForCausalLM)::from_pretrained` or `(PythonCausalLM::new|PythonDistributedCausalLM::new)`.

We currently only support causal language models — to implement a new one, you can create a file similar to `llama_for_causal_lm` and implement your model, ensuring you provide a trait impl for `CausalLM` - or, preferrably, add your model to [our TorchTitan fork](https://github.com/nousResearch/torchtitan). See the [Python](./python.md) docs for more information.

You might also need to modify the data provider if the data your model requires is structured in some way.
Since you're implementing the forward pass yourself, you can serve and interpret data passed from the data provider however you need.
The data provider currently only supports reading fixed-size batches from input files, so data batches with different sizes will require some additional work.

> PRs welcome for any new kinds of dataset loading!
