# Implementing models

This codebase includes a set of sample programs that let you design, implement, and test model architectures without spinning up the whole Psyche p2p training architecture.

We currently only implement Llama and Deepseek (see `shared/modeling/src/models/`), but PRs are very welcome to add more architectures and model types.

The `train` example, documented below, is useful to test how your model trains using AdamW vs DisTrO.

## Running

```bash
cargo run --example train -- --help
```

You'll need a pre-tokenized dataset for training. The `train` example supports multiple data sources: local files, HTTP URLs, GCP buckets, and weighted configurations.

For a Llama 2 model, a pre-tokenized dataset to test with is available at [https://huggingface.co/datasets/emozilla/fineweb-10bt-tokenized-datatrove-llama2/](https://huggingface.co/datasets/emozilla/fineweb-10bt-tokenized-datatrove-llama2/tree/main).
Psyche only needs the `.ds` files, and will load any/all `.ds` files in the specified folder - you can use just one for smaller tests.

### Local data

If you've downloaded part or all of the above dataset into a folder `data/fineweb-10bt` inside the Psyche repo, you can start a simple training run on a 20m parameter Llama 2 model:

```bash
cargo run --example train -- \
    --model emozilla/llama2-20m-init \
    --data-path ./data/fineweb-10bt/ \
    --total-batch 2 \
    --micro-batch 1
```

#### Local preprocessed data

For preprocessed data in parquet format (with `inputs` column), use `local-preprocessed`:

```bash
cargo run --example train -- \
    --model emozilla/llama2-20m-init \
    --total-batch 2 \
    --micro-batch 1 \
    local-preprocessed --path ./data/parquet/
```

### HTTP

You can stream data directly from HTTP URLs without downloading the dataset first. There are several ways to specify HTTP data sources:

#### URL template

Use a template with `{}` placeholder that gets replaced with padded numbers:

```bash
cargo run --example train -- \
    --model emozilla/llama2-20m-init \
    --total-batch 2 \
    --micro-batch 1 \
    http-template \
        --template "https://example.com/data/{}.ds" \
        --start 0 \
        --end 10 \
        --left-pad-zeros 5
```

This would load files from `https://example.com/data/00000.ds` through `https://example.com/data/00009.ds`.

#### Explicit URLs

Provide a list of URLs directly:

```bash
cargo run --example train -- \
    --model emozilla/llama2-20m-init \
    --total-batch 2 \
    --micro-batch 1 \
    urls \
        https://example.com/data/file1.ds \
        https://example.com/data/file2.ds
```

#### GCP bucket

Load all `.ds` files from a Google Cloud Storage bucket:

```bash
cargo run --example train -- \
    --model emozilla/llama2-20m-init \
    --total-batch 2 \
    --micro-batch 1 \
    gcp \
        --bucket-name my-bucket \
        --directory data/tokenized
```

## Adding a new model type

The `train` example currently asssumes your model is a Llama or Deepseek v2/v3 model, and instantiates it via `(LlamaForCausalLM|DeepseekForCausalLM)::from_pretrained`.

We currently only support causal language models - to implement a new one, you can create a file similar to `llama_for_causal_lm` and implement your model, ensuring you provide a trait impl for `CausalLM`.

There's alpha-level support for models written in Python. See the [Python](./python.md) docs for more information.

You might also need to modify the data provider, if your data is structured in some way.
Since you're implementing the forward pass yourself, you can serve and interpret data passed from the data provider however you need.
The data provider currently only supports reading fixed-size batches from input files, so data batches with different sizes will require some additional work.

> PRs welcome for any new kinds of dataset loading!
