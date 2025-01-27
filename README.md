# psyche

<p align="center" width="100%">
    <img src="./psyche.jpg"> 
</p>

For more detailed documentation on the Psyche project, please visit [the Psyche docs](https://docs.psyche.network).

This codebase contains the model training code for the Psyche project.
It's written in Rust and uses the [Torch](https://pytorch.org/) library for training.

It's designed to let you modify and test model architectures, for near-future inclusion in the Psyche testnet.

We currently only implement Llama, but PRs are very welcome to add more architectures and model types.

The `train` example listed below is useful to test how your model trains using AdamW vs DisTrO.

## Running

`$ cargo run --example train -- ---help`

You'll need a pre-tokenized dataset downloaded to your disk for training.

> A PR is welcome to add an option to the trainer to use the HTTP data provider! You can refer to the http example in the data-provider crate for a sample implementation.

For a Llama 2 model, a pre-tokenized dataset to test with is available at [https://huggingface.co/datasets/emozilla/fineweb-10bt-tokenized-datatrove-llama2/](https://huggingface.co/datasets/emozilla/fineweb-10bt-tokenized-datatrove-llama2/tree/main). Psyche only needs the `.ds` files, and will load any in the specified folder - you can download just one for smaller tests.

If you've downloaded part or all of the above dataset into a folder `data/fineweb-10bt` inside the Psyche repo, you can start a simple training run on a 20m parameter Llama 2 model:

`$ cargo run --example train -- --model emozilla/llama2-20m-init --data-path ./data/fineweb-10bt/ --total-batch 2 --micro-batch 1`

## Adding a new model type
The `train` example currently assumes your model is a Llama model, and instantiates it via `LlamaForCausalLM::from_pretrained`.

We currently only support causal language models - to implement a new one, you can create a file similar to `llama_for_causal_lm` and implement your model, ensuring you provide the trait impls `CausalLM` and `ConcreteCausalLM`. `ConcreteCausalLM` isn't used in this repo, but will be needed for more advanced parallelism in the full Psyche codebase.

You might also need to modify the data provider, if your data is structured in some way.

Since you're implementing the forward pass yourself, you can serve and interpret data passed from the data provider however you need.
The data provider currently only supports reading fixed-size batches from input files, so data batches with different sizes will require some additional work.


## Setup

### Ubuntu

The following instructions are needed for a server with a fresh Ubuntu installation

1. Install drivers

```bash
sudo apt update
sudo apt install -y ubuntu-drivers-common
sudo ubuntu-drivers install
```

2. Install CUDA libraries

```bash
wget https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2204/x86_64/cuda-keyring_1.1-1_all.deb
sudo dpkg -i cuda-keyring_1.1-1_all.deb
sudo apt-get update
sudo apt-get -y install cuda-toolkit-12-4
rm cuda-keyring_1.1-1_all.deb
sudo apt-get install libnccl-dev libnccl2
sudo apt install nvidia-cuda-toolkit
```

3. Download libtorch & extract

```bash
wget https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip
unzip libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip
rm libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip
```

4. In the `.bashrc` file, set the following libtorch environment variables. Here `<path_to_libtorch>` is the absolute path
   to the extracted `libtorch` folder from the previous step

```bash
export LIBTORCH=<path_to_libtorch>
export LIBTORCH_INCLUDE=<path_to_libtorch>
export LIBTORCH_LIB=<path_to_libtorch>
export LD_LIBRARY_PATH=<path_to_libtorch>/lib:$LD_LIBRARY_PATH
```

This can also be acheived by making a `.cargo/config.toml` file in the checkout path

```
[env]
LIBTORCH=<path_to_libtorch>
LD_LIBRARY_PATH=<path_to_libtorch>/lib
CUDA_ROOT = "/usr/local/cuda-12.4"
```

5. Download & install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### NixOS

#### Direnv

0. Install `direnv`
1. `direnv allow`

#### Non-direnv

`nix develop` to enter a development shell

### Windows

1. Install CUDA libraries: https://developer.nvidia.com/cuda-12-4-1-download-archive?target_os=Windows&target_arch=x86_64&target_version=11

2. Download libtorch & extract: https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.4.1%2Bcu124.zip

3. Download OpenSSL: https://slproweb.com/download/Win64OpenSSL-3_3_2.exe

4. Install Perl: https://github.com/StrawberryPerl/Perl-Dist-Strawberry/releases/download/SP_53822_64bit/strawberry-perl-5.38.2.2-64bit.msi

5. Create a `.cargo/config.toml` file to set environment variables

**NOTE**: Building may take several minutes the first time as `openssl-sys` takes a long time (for some reason)

```
[env]
LIBTORCH = <path_to_libtorch>
OPENSSL_LIB_DIR = <path_to_openssl>/lib/VC/x64/MT
OPENSSL_INCLUDE_DIR <path_to_openssl>/include
```
