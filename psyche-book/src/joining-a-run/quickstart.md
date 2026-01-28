# Quickstart

This guide will walk you through joining a Psyche training run from scratch.

## Before You Begin

Make sure you have:

- A Linux system with an NVIDIA GPU
- NVIDIA drivers installed
- Docker and NVIDIA Container Toolkit installed
- Access to a Solana RPC provider (see [Requirements](./requirements.md) for details)

## Step 1: Verify Your Setup

Test that Docker can access your GPU:

```bash
docker run --rm --gpus all nvidia/cuda:11.8.0-base-ubuntu22.04 nvidia-smi
```

You should see your GPU listed in the output. If you get errors, review the [Requirements](./requirements.md) page.

## Step 2: Get Your Solana Wallet

If you don't already have a Solana wallet, create one:

```bash
solana-keygen new -o ~/psyche-wallet.json
```

**Important:** Save the seed phrase shown - this is your only way to recover the wallet.

Get your public key:

```bash
solana-keygen pubkey ~/psyche-wallet.json
```

### Check Authorization

Verify you're authorized to join the run (replace `<RUN_ID>`, `<AUTHORIZER>`, and `<PUBKEY>` with your values):

```bash
psyche-solana-client can-join \
    --run-id <RUN_ID> \
    --authorizer <AUTHORIZER> \
    --wallet <PUBKEY>
```

If this command succeeds, you're authorized. If not, contact the run owner to get authorized.

## Step 3: Create Your Configuration File

Create a `.env` file with your settings. Here's a template:

```bash
# Create .env file
cat > ~/psyche-client.env << 'EOF'
# === Solana RPC Configuration ===
# Your primary RPC provider (get from Helius, QuickNode, etc.)
RPC=https://your-rpc-provider.com
WS_RPC=wss://your-rpc-provider.com

# Backup RPC provider (can use public endpoints)
RPC_2=https://api.devnet.solana.com
WS_RPC_2=wss://api.devnet.solana.com

# === Run Configuration ===
# The ID of the training run you're joining (optional)
# If not specified, the client will automatically discover and join an available run
RUN_ID=your-run-id

# === GPU Configuration ===
# Enable all NVIDIA capabilities
NVIDIA_DRIVER_CAPABILITIES=all

# GPU Parallelism Settings
# DATA_PARALLELISM: Number of GPUs to split data across (1 if you have 1 GPU)
DATA_PARALLELISM=1

# TENSOR_PARALLELISM: Number of GPUs to split the model across (1 if you have 1 GPU)
TENSOR_PARALLELISM=1

# MICRO_BATCH_SIZE: Samples processed per GPU per step
# Start with 4, increase if you have VRAM to spare, decrease if you get OOM errors
MICRO_BATCH_SIZE=4

# === Authorization ===
# The Solana address that authorized you to join this run
AUTHORIZER=your-authorizer-pubkey
EOF
```

**Edit this file** and replace:

- `https://your-rpc-provider.com` - Your RPC provider URL
- `wss://your-rpc-provider.com` - Your RPC provider WebSocket URL
- `your-run-id` - The run ID you want to join (or omit to auto-discover runs)
- `your-authorizer-pubkey` - The authorizer's public key

### Automatic Run Discovery

If you don't specify a `RUN_ID`, the client will automatically query available runs from the coordinator, filter out inactive runs (Uninitialized, Finished, or Paused), and join the first available active run. The selected run will be displayed in the logs.

### GPU Configuration Guidance

**For 1 GPU systems:**

- `DATA_PARALLELISM=1`
- `TENSOR_PARALLELISM=1`
- Adjust `MICRO_BATCH_SIZE` based on your VRAM

**For multi-GPU systems:**

- Increase `DATA_PARALLELISM` to split data across GPUs (faster training)
- Increase `TENSOR_PARALLELISM` if the model doesn't fit on one GPU
- Example with 2 GPUs: `DATA_PARALLELISM=2`, `TENSOR_PARALLELISM=1`

## Step 4: Run the Psyche Client

Pull the latest Docker image:

```bash
docker pull nousresearch/psyche-client:latest
```

Start your client:

```bash
docker run -d \
    --name psyche-client \
    --env-file ~/psyche-client.env \
    -e RAW_WALLET_PRIVATE_KEY="$(cat ~/psyche-wallet.json)" \
    --gpus all \
    --network "host" \
    nousresearch/psyche-client:latest
```

**Explanation of flags:**

- `-d` - Run in detached mode (background)
- `--name psyche-client` - Name the container for easy reference
- `--env-file` - Load environment variables from your .env file
- `-e RAW_WALLET_PRIVATE_KEY` - Pass your wallet private key
- `--gpus all` - Give container access to all GPUs
- `--network "host"` - Use host networking for P2P communication

## Step 5: Monitor Your Client

Check if the container is running:

```bash
docker ps | grep psyche-client
```

View logs:

```bash
docker logs -f psyche-client
```

**What you should see:**

- Client connecting to RPC endpoints
- Downloading model checkpoint (first time only)
- Entering warmup phase
- Starting training rounds

**Press Ctrl+C to stop following logs** (container keeps running).

## Step 6: Verify Training Progress

Look for these log messages indicating successful training:

```
[INFO] Coordinator state: Warmup
[INFO] Downloading model checkpoint...
[INFO] Coordinator state: RoundTrain
[INFO] Training on batch assignments...
[INFO] Completed training round X
```

If you see these messages, congratulations! You're successfully participating in the training run.

## Managing Your Client

**Stop the client:**

```bash
docker stop psyche-client
```

**Restart the client:**

```bash
docker start psyche-client
```

**Remove the client (to start fresh):**

```bash
docker rm -f psyche-client
```

## Claiming Rewards

Once you've accumulated rewards, claim them with:

```bash
psyche-solana-client treasurer-claim-rewards \
    --rpc <RPC> \
    --run-id <RUN_ID> \
    --wallet-private-key-path ~/psyche-wallet.json
```

## Troubleshooting

If you encounter issues:

1. **Container won't start** - Check Docker logs: `docker logs psyche-client`
2. **GPU not detected** - Verify NVIDIA Container Toolkit installation
3. **Out of memory errors** - Reduce `MICRO_BATCH_SIZE` in your .env file
4. **RPC connection failures** - Verify your RPC URLs are correct
5. **Not authorized** - Confirm with run owner that your wallet is authorized

For detailed troubleshooting, see [Troubleshooting](./troubleshooting.md).

## What's Next

- Understand what's happening behind the scenes: [Workflow Overview](../explain/workflow-overview.md)
- Learn about the training phases: [Run States](../explain/run-states.md)
- Read the [FAQ](./faq.md) for common questions
- Check your rewards and contribution: Monitor the logs or use the treasurer commands

## Updating Your Client

To update to the latest version:

```bash
docker stop psyche-client
docker pull nousresearch/psyche-client:latest
docker start psyche-client
```

The client will automatically resume from where it left off.
