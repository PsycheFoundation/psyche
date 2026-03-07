# Setup & Useful Commands

## Installation and Setup

Psyche uses `nix` + flakes to install every single dependency and development tool Psyche needs to run and be developed.
This is the preferred way of working on Psyche, as it guarantees a consistent development and build process regardless of your machine's specific configuration.

If you can't / don't want to use Nix, it's also possible to manually install all the required deps for Psyche.

### Linux & macOS via Nix

#### Installing Nix

To install Nix, simply run the `./setup-nix.sh` script. This will install Nix and configure it appropriately.

##### Binary cache

To speed up your builds and your local dev shell, we recommend enabling the binary cache from `garnix`, our CI provider.

In order to use the cache that garnix provides, change your `nix.conf`, adding `https://cache.garnix.io` to substituters, and `cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g=` to `trusted-public-keys`.

If you've just installed Nix via the Determinate Systems installer above, you can do this by adding these lines to `/etc/nix/nix.conf`:

```conf
extra-substituters = https://cache.garnix.io
extra-trusted-public-keys = cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g=
```

#### Setup

Each time you open a new shell in the Psyche directory, run `nix develop` to enter the Psyche development shell with all the necessary dependencies.

#### Setup Using `direnv`

You can optionally use `direnv` to automatically enter a Nix environment when you `cd` into the Psyche folder.

1. Install `direnv` from your system's package manager:

- `sudo apt install direnv` on Debian-based systems
- `brew install direnv` on macOS

2. Run `nix profile install nixpkgs#nix-direnv` to install the `direnv` plugin in nix.
3. Run `echo "source ~/.nix-profile/share/nix-direnv/direnvrc" > ~/.direnvrc` to enable the plugin.
4. Add `eval "$(direnv hook bash)"` line to your shell configuration file (e.g., `~/.bashrc` or `~/.zshrc`).
5. Run `direnv allow` in the Psyche directory once, your terminal will automatically enter a development shell when you subsequently `cd` into the Psyche directory.

#### Platform Differences

- **Linux**: Uses CUDA for NVIDIA GPUs. Full distributed training support with NCCL.
- **macOS**: Uses Metal Performance Shaders for Apple Silicon GPUs. Single-GPU only (parallelism features disabled).

### Ubuntu

The following instructions are needed for a server with a fresh Ubuntu installation

#### 1. Install drivers (if not already installed)

```bash
sudo apt update
sudo apt install -y ubuntu-drivers-common
sudo ubuntu-drivers install
```

#### 2. Create and enter a Python virtual env

```bash
sudo apt install -y python3-pip python3-venv
python3 -m venv .venv
source .venv/bin/activate
```

#### 3. Install Torch 2.7.0 CUDA 12.8

```bash
pip3 install torch==2.7.0 --index-url https://download.pytorch.org/whl/cu128
```

#### 4. Libtorch environment variables

Add the following section to `.cargo/config.toml`. Adjust `LD_LIBRARY_PATH` for your `<repo_directory>` and specific version of Python (3.10 shown here). **NOTE: Don't commit these changes!**

```toml
[env]
LIBTORCH_USE_PYTORCH = "1"
LD_LIBRARY_PATH = "<repo_directory>/.venv/lib/python3.10/site-packages/torch/lib"
```

#### 5. Download & install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

#### 6. (optional) Install `just`

```bash
sudo snap install just --edge --classic
```

#### 7. (optional) Install Solana and Anchor

Install Solana

```bash
sh -c "$(curl -sSfL https://release.anza.xyz/beta/install)"
```

After installation, follow the instructions to add the Solana tools to PATH.

Install Anchor

```bash
cargo install --git https://github.com/coral-xyz/anchor --rev a7a23eea308440a9fa9cb79cee7bddd30ab163d5 anchor-cli
```

This may require

```bash
sudo apt install pkg-config libudev-dev libssl-dev libfontconfig-dev
```

### Windows (outdated)

1. Install CUDA libraries: https://developer.nvidia.com/cuda-12-4-1-download-archive?target_os=Windows&target_arch=x86_64&target_version=11

2. Download libtorch & extract: https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.6.0%2Bcu124.zip

3. Download OpenSSL: https://slproweb.com/download/Win64OpenSSL-3_3_3.exe

4. Install Perl: https://github.com/StrawberryPerl/Perl-Dist-Strawberry/releases/download/SP_53822_64bit/strawberry-perl-5.38.2.2-64bit.msi

5. Create a `.cargo/config.toml` file to set environment variables

**NOTE**: Building may take several minutes the first time as `openssl-sys` takes a long time (for some reason)

```
[env]
LIBTORCH = <path_to_libtorch>
OPENSSL_LIB_DIR = <path_to_openssl>/lib/VC/x64/MT
OPENSSL_INCLUDE_DIR = <path_to_openssl>/include
```

## Useful commands

Psyche uses [`just`](https://github.com/casey/just) to run some common tasks.
You can run `just` to see the whole list of available commands!

### Running checks

```bash
just check-client
```

Will run the psyche-solana-client package with the `--help` flag. If you see a list of commands, it means that the env compiles and can run the basic commands.

### Formatting

> requires Nix!

```bash
nix fmt
```

Format all the project files.
