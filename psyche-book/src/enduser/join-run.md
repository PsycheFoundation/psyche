# Joining a training run

This guide is for end-users who wish to participate in a training run, it assumes you have a predistributed `run-manager` binary. If you are looking for more in-depth documentation or how to run from source you can refer to the [development documentation](../development/index.md)

## Prerequisites

Before joining a run you need to make sure you meet a few requisites:

### Linux Operating System

The Psyche client currently only runs on modern Linux distributions.

### NVIDIA GPU and Drivers

Psyche requires an NVIDIA CUDA-capable GPU for model training. Your system must have NVIDIA drivers installed.

To check if you have NVIDIA drivers:

```bash
nvidia-smi
```

If this command doesn't work or shows an error, you need to install NVIDIA drivers. Follow NVIDIA's [installation guide](https://docs.nvidia.com/datacenter/tesla/driver-installation-guide/) for your Linux distribution.

### Docker

The Psyche client runs inside a Docker container. You need Docker Engine installed on your system.

To check if Docker is installed:

```bash
docker --version
```

If Docker isn't installed, follow the [Docker Engine installation guide](https://docs.docker.com/engine/install/) for your Linux distribution.

### NVIDIA Container Toolkit

The NVIDIA Container Toolkit is required to enable GPU access inside Docker containers, which Psyche uses for model training.

To install the NVIDIA Container Toolkit, follow the [NVIDIA Container Toolkit installation guide](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) for your Linux distribution.

### Solana Wallet/Keypair

You need a Solana keypair (wallet) to participate in training. This keypair identifies your client on the blockchain.

If you need to create a new keypair, you can use the Solana CLI, specifying where you want to create it

```bash
solana-keygen new --outfile <path/to/keypair/file.json>
```

## Quick Start

The recommended way to run a Psyche client is through the `run-manager`, which should have been distributed to you. The run manager will handle downloading the correct Docker image, starting your client, and keeping it updated automatically.
Before running it, you should create an environment file with some needed variables.

The `.env` file should have at least this defined:

```bash
WALLET_PATH=/path/to/your/keypair.json

# Required: Solana RPC Endpoints
RPC=https://your-primary-rpc-provider.com
WS_RPC=wss://your-primary-rpc-provider.com

# Required: Which run id to join
RUN_ID=your_run_id_here

# Recommended: Fallback RPC Endpoints (for reliability)
RPC_2=https://your-backup-rpc-provider.com
WS_RPC_2=wss://your-backup-rpc-provider.com
```

Then, you can start training through the run manager running:

```bash
./run-manager --env-file /path/to/your/.env
```

After the initial setup, you'll see the Psyche client logs streaming in real-time. These logs show training progress, network status, and other important information.

To stop the client, press `Ctrl+C` in the terminal.

## RPC Hosts

We recommend using a dedicated RPC service such as [Helius](https://www.helius.dev/), [QuickNode](https://www.quicknode.com/), [Triton](https://triton.one/), or self-hosting your own Solana RPC node.

## Additional config variables

In general it's not neccesary to change these variables to join a run since we provide sensible defaults,
though you might need to.

**`NVIDIA_DRIVER_CAPABILITIES`** - An environment variable that the NVIDIA Container Toolkit uses to determine which compute capabilities should be provided to your container. It is recommended to set it to 'all', e.g. `NVIDIA_DRIVER_CAPABILITIES=all`.

**`PARALLELISM_AUTO`** - Set to `true` to automatically detect optimal parallelism settings based on the model and your GPU hardware.

- When enabled, the client will look up the best `DATA_PARALLELISM`, `TENSOR_PARALLELISM`, and `MICRO_BATCH_SIZE` values from a [built-in configuration table](https://github.com/PsycheFoundation/psyche/blob/main/shared/client/src/parallelism_data.json)
- Your model and GPU hardware combination must be present in the table
- This is the recommended option for most users
- If set, manual parallelism settings below will be ignored

**`DATA_PARALLELISM`** - Number of GPUs to distribute training data across.

- If you have multiple GPUs, you can set this to 2, 4, etc. to speed up training
- If you have 1 GPU, set this to `1`
- Ignored if `PARALLELISM_AUTO=true`

**`TENSOR_PARALLELISM`** - Number of GPUs to distribute the model across, this lets you train a model you can't fit on one single GPU.

- If you have 1 GPU, set this to `1`
- If your have `n` GPUs you can distribute the model across all of them by setting it to `n`.
- Ignored if `PARALLELISM_AUTO=true`

**`MICRO_BATCH_SIZE`** - Number of samples processed per GPU per training step

- Set as high as your GPU memory allows
- Ignored if `PARALLELISM_AUTO=true`

**`AUTHORIZER`** - The Solana address that authorized your wallet to join this run

- See [Authentication](./authentication.md) for more details

## Testing Authorization

Before joining a run, you can verify that your client is authorized by using the `run-manager` command:

```bash
run-manager can-join --run-id <RUN_ID> --authorizer <AUTHORIZER> --address <PUBKEY>
```

Where:

- `<RUN_ID>` is the run ID you want to join (from your `.env` file)
- `<AUTHORIZER>` is the Solana authorizer address (from your `.env` file)
- `<PUBKEY>` is your wallet's public key

You can find your wallet's public key by running:

```bash
solana address
```

This command will return successfully if your wallet is authorized to join the run. This helps debug authorization issues before attempting to join.

## Troubleshooting

### Docker Not Found

**Error:** `Failed to execute docker command. Is Docker installed and accessible?`

**Solution:** Install Docker using the [Docker installation guide](https://docs.docker.com/engine/install/). Make sure your user is in the `docker` group:

```bash
sudo usermod -aG docker $USER
```

Then log out and back in for the group change to take effect.

### NVIDIA Drivers Not Working

**Error:** Container starts but crashes immediately, or you see GPU-related errors in logs

**Solution:**

- Verify drivers are installed: `nvidia-smi`

### RPC Connection Failures

**Error:** `RPC error: failed to get account` or connection timeouts

**Solution:**

- Verify your RPC endpoints are correct in your `.env` file
- Check that your RPC provider API key is valid, if present
- Try your backup RPC endpoints (`RPC_2`, `WS_RPC_2`)

### Wallet/Keypair Not Found

**Error:** `Failed to read wallet file from: /path/to/keypair.json`

**Solution:**

- Verify the file exists: `ls -l /path/to/keypair.json`
- Check file permissions: `chmod 600 /path/to/keypair.json`
- If using default location, ensure `~/.config/solana/id.json` exists
- Verify the path in your `.env` file matches the actual file location

### Container Fails to Start

**Error:** `Docker run failed: ...` or container exits immediately

**Solution:**

- Check Docker logs for more details
- Ensure all required variables are in your `.env` file
- Verify GPU access: `docker run --rm --gpus all ubuntu nvidia-smi`
- Check disk space: `df -h`
- Verify you have enough VRAM for your `MICRO_BATCH_SIZE` setting (try reducing it)

### Process Appears Stuck

**Error:** No new logs appearing, process seems frozen, stuck with error messages.

**Solution:**

- The run manager will attempt to restart the client, but sometimes this can fail and hang.
- Press `Ctrl+C` to stop run-manager and wait a few seconds.
- If for some reason this fails to stop it, you can check the running containers with `docker ps`
  and force stop the container manually with `docker stop`.

### Version Mismatch Loop

**Symptom:** Container keeps restarting every few seconds with "version mismatch"

**Solution:**

- This usually means there's an issue with pulling the new Docker image
- Check your internet connection
- Verify Docker can be run `docker --version`
- Verify Docker Hub is accessible: `docker pull hello-world`
- Check disk space for Docker images: `docker system df`

### Checking Container Logs Manually

If run-manager exits but you want to see what happened, you can view Docker logs:

```bash
# List recent containers (including stopped ones)
docker ps -a

# View logs for a specific container
docker logs CONTAINER_ID
```

## Claiming Rewards

After participating in training and accumulating rewards, you can claim them using the `run-manager` command:

```bash
run-manager treasurer-claim-rewards \
    --rpc <RPC> \
    --run-id <RUN_ID> \
    --wallet-private-key-path <JSON_PRIVATE_KEY_PATH>
```

Where:

- `<RPC>` is your Solana RPC endpoint (same as in your `.env` file)
- `<RUN_ID>` is the run ID you participated in
- `<JSON_PRIVATE_KEY_PATH>` is the path to your wallet keypair file (e.g., `~/.config/solana/id.json`)

This command will claim any rewards you've earned from contributing to the training run.

## Building from source

If you wish to run the run-manager from source, first make sure that you have followed the [development setup](../development/setup.md), are inside the `nix` environment, and run `just run-manager path/to/.env.file`
