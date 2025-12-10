# Implementing models

This codebase includes a set of sample programs that let you design, implement, and test model architectures without spinning up the whole Psyche p2p training architecture.

We currently only implement Llama and Deepseek (see `shared/modeling/src/models/`), but contributions are very welcome to add more architectures and model types.

The `train` example, documented below, is useful to test how your model trains using AdamW vs DisTrO.

To run the options of the `train` example, use the following command:

```bash
just train-model --help
```

We'll need some pre-tokenized to use for the model to train, the example supports downloading and using the data locally or use a HuggingFace dataset repo to get all the dataset files and host an http provider to pull all the data.

For a Llama 2 model, a pre-tokenized dataset to test with is available at [https://huggingface.co/datasets/emozilla/fineweb-10bt-tokenized-datatrove-llama2/](https://huggingface.co/datasets/emozilla/fineweb-10bt-tokenized-datatrove-llama2/tree/main).

Psyche only needs the `.ds` files, and will load any/all `.ds` files in the specified folder - you can download just one for smaller tests but to download all files, you can use the HuggingFace CLI and place it in a folder named `data/fineweb-10bt`:

```bash
hf download emozilla/fineweb-10bt-tokenized-datatrove-llama2 --repo-type dataset --local-dir ./data/fineweb-10bt
```

And then train a basic model with:

```bash
just train-model \
    --model emozilla/llama2-20m-init \
    --data-path ./data/fineweb-10bt/ \
    --total-batch 2 \
    --micro-batch 1
```

Alternatively to use the data with the HTTP provider you can run:

```bash
just train-model \
    --model emozilla/llama2-20m-init \
    --data-provider http \
    --data-url emozilla/fineweb-10bt-tokenized-datatrove-llama2 \
    --total-batch 2 \
    --micro-batch 1
```

That will get all the `.ds` files in the HuggingFace repo and load them into the HTTP provider to be requested in the `train` example and use it to train the model. Both examples work with the same model and the same exact data.

## Adding a new model type

The `train` example currently assumes your model is a Llama or Deepseek v2/v3 model, and instantiates it via `(LlamaForCausalLM|DeepseekForCausalLM)::from_pretrained`.

We currently only support causal language models - to implement a new one, you can create a file similar to `llama_for_causal_lm` and implement your model, ensuring you provide a trait impl for `CausalLM`.

There's alpha-level support for models written in Python. See the [Python](./python.md) docs for more information.

You might also need to modify the data provider, if your data is structured in some way.
Since you're implementing the forward pass yourself, you can serve and interpret data passed from the data provider however you need.
The data provider currently only supports reading fixed-size batches from input files, so data batches with different sizes will require some additional work.

> PRs welcome for any new kinds of dataset loading!
