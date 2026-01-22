# Quickstart: Providing Compute to NousNet

This guide walks you through the complete process of setting up your machine to provide compute to a NousNet training run. It assumes you have been provided the `run-manager` binary by the run administrator.

## Prerequisites Checklist

Before starting, ensure you have:

- [ ] Linux operating system (Ubuntu recommended)
- [ ] NVIDIA GPU with sufficient VRAM for the model being trained
- [ ] The `run-manager` binary
- [ ] Run ID from the run administrator

---

## Step 1: Verify NVIDIA Drivers

NousNet requires an NVIDIA CUDA-capable GPU. Verify your drivers are installed:

```bash
nvidia-smi
```

You should see output showing your GPU model, driver version, and CUDA version. If this command fails, install NVIDIA drivers following the [NVIDIA driver installation guide](https://docs.nvidia.com/datacenter/tesla/driver-installation-guide/).

---

## Step 2: Install Docker

Install Docker Engine following the [official Docker installation guide](https://docs.docker.com/engine/install/) for your Linux distribution.

After installation, verify Docker is working:

```bash
docker --version
```

### Docker Post-Installation Steps

**Important:** You must add your user to the `docker` group to run Docker without `sudo`:

```bash
sudo usermod -aG docker $USER
```

Then **log out and back in** (or reboot) for the group change to take effect.

Verify the change worked:

```bash
docker run hello-world
```

If this runs without requiring `sudo`, you're set.

For more details, see the [Docker post-installation guide](https://docs.docker.com/engine/install/linux-postinstall/).

---

## Step 3: Install NVIDIA Container Toolkit

The NVIDIA Container Toolkit enables GPU access inside Docker containers. This is required for NousNet to use your GPU for training.

Follow the [NVIDIA Container Toolkit installation guide](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) for your distribution.

After installation, verify GPU access works inside Docker:

```bash
docker run --rm --gpus all nvidia/cuda:12.2.2-devel-ubuntu22.04
```

You should see the same GPU information as running `nvidia-smi` directly.

> **Troubleshooting:** If you see an error like `could not select device driver "" with capabilities: [[gpu]]`, the NVIDIA Container Toolkit is not installed correctly. Revisit the installation guide.

---

## Step 4: Install Solana CLI and Create Wallet

### Install Solana CLI

```bash
sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
```

After installation, add Solana to your PATH by adding this line to your `~/.bashrc` or `~/.zshrc`:

```bash
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
```

Then reload your shell:

```bash
source ~/.bashrc  # or source ~/.zshrc
```

Verify the installation:

```bash
solana --version
```

For more details, see the [Solana installation docs](https://solana.com/docs/intro/installation).

### Generate a Keypair

Create a new Solana keypair for your node:

```bash
solana-keygen new --outfile ~/.config/solana/psyche-node.json
```

You'll be prompted to set an optional passphrase. The keypair file will be created at the specified path.

**Important:** Back up this keypair file securely. If you lose it, you lose access to any rewards earned.

Get your public key (you'll need this):

```bash
solana-keygen pubkey ~/.config/solana/psyche-node.json
```

---

## Step 5: Get Authorization to Join the Run

NousNet runs are permissioned. To join, you need the run administrator to authorize your wallet.

1. **Send your public key to the run administrator** (the output from `solana-keygen pubkey` above)
2. The administrator will create an authorization for your key
3. Once authorized, you can proceed to join the run

---

## Step 6: Fund Your Wallet (Devnet)

Your wallet needs SOL to pay for transaction fees when communicating with the Solana blockchain.

First, configure Solana CLI to use devnet:

```bash
solana config set --url https://api.devnet.solana.com
```

Then request an airdrop from the devnet faucet:

```bash
solana airdrop 2 ~/.config/solana/psyche-node.json
```

Verify your balance:

```bash
solana balance ~/.config/solana/psyche-node.json
```

> **Note:** If the airdrop fails due to rate limiting, wait a few minutes and try again, or use the [Solana Faucet web interface](https://faucet.solana.com/).

---

## Step 7: Create the Environment File

Create a `.env` file with your configuration. This file tells the run-manager how to connect and authenticate.

```bash
# Create the env file
cat > ~/.config/psyche/run.env << 'EOF'
# Path to your Solana keypair
WALLET_PRIVATE_KEY_PATH=/home/YOUR_USERNAME/.config/solana/psyche-node.json

# Solana RPC endpoints (devnet)
RPC=https://api.devnet.solana.com
WS_RPC=wss://api.devnet.solana.com

# The run you're joining (provided by run administrator)
RUN_ID=your_run_id_here

# Your public key (the one authorized by the run admin)
AUTHORIZER=YOUR_PUBLIC_KEY_HERE

# Required for GPU access in container
NVIDIA_DRIVER_CAPABILITIES=all
EOF
```

**Replace the following values:**

| Variable               | Replace With                       |
| ---------------------- | ---------------------------------- |
| `YOUR_USERNAME`        | Your Linux username                |
| `your_run_id_here`     | The run ID from your administrator |
| `YOUR_PUBLIC_KEY_HERE` | Your wallet's public key           |

### Optional Configuration

You can add these optional variables to tune performance, please ask run adminstrator for help:

```bash
# Number of GPUs to use for data parallelism (default: 1)
DATA_PARALLELISM=1

# Number of GPUs to distribute model across (default: 1)
TENSOR_PARALLELISM=1

# Samples per GPU per training step (tune based on VRAM)
MICRO_BATCH_SIZE=4
```

---

## Step 8: Run the Manager

Make the binary executable if needed:

```bash
chmod +x ./run-manager
```

Open and enter a tmux window:

```bash
tmux
```

Start providing compute to the network:

```bash
./run-manager --env-file ~/.config/psyche/run.env
```

The run-manager will:

1. Connect to the Solana coordinator
2. Pull the appropriate Docker image for the run
3. Start the training container
4. Stream logs to your terminal

---

## Step 9: Verify It's Working

After starting, you should see:

1. **Image pull progress** - Docker downloading the NousNet client image
2. **Container startup** - The training container initializing
3. **Connection logs** - Your client connecting to the coordinator
4. **Training logs** - Progress updates as training proceeds

A healthy startup looks something like:

```
INFO run_manager: Docker tag for run 'your_run': nousresearch/psyche-client:v0.x.x
INFO run_manager: Pulling image from registry: nousresearch/psyche-client:v0.x.x
INFO run_manager: Starting container...
INFO run_manager: Started container: abc123...
[+] Starting to train in run your_run...
```

To stop the client gracefully, press `Ctrl+C`.

---

## Troubleshooting

### GPU Not Detected in Container

**Error:** `could not select device driver "" with capabilities: [[gpu]]`

**Solution:** The NVIDIA Container Toolkit is not installed or configured correctly. Revisit Step 3 and ensure you can run `docker run --rm --gpus all nvidia/cuda:12.0-base nvidia-smi` successfully.

### Docker Permission Denied

**Error:** `permission denied while trying to connect to the Docker daemon socket`

**Solution:** Your user isn't in the `docker` group. Run:

```bash
sudo usermod -aG docker $USER
```

Then **log out and back in**.

### Wallet Not Found

**Error:** `Failed to read wallet file from: /path/to/keypair.json`

**Solution:** Verify the `WALLET_PRIVATE_KEY_PATH` in your `.env` file points to an existing file:

```bash
ls -l ~/.config/solana/psyche-node.json
```

### RPC Connection Failures

**Error:** `RPC error: failed to get account` or connection timeouts

**Solution:**

- Verify your RPC endpoints are correct in the `.env` file
- For devnet, use `https://api.devnet.solana.com` and `wss://api.devnet.solana.com`
- The public devnet RPC has rate limits; if issues persist, consider using a dedicated RPC provider

### Not Authorized to Join

**Error:** Authorization or permission errors when trying to join

**Solution:** Confirm with the run administrator that your public key has been authorized. You can verify your authorization status:

```bash
./run-manager can-join \
    --rpc https://api.devnet.solana.com \
    --run-id YOUR_RUN_ID \
    --authorizer YOUR_PUBLIC_KEY \
    --address YOUR_PUBLIC_KEY
```

### Container Keeps Restarting

**Symptom:** Container restarts repeatedly with "version mismatch"

**Solution:** This usually indicates a Docker image pull issue:

- Check your internet connection
- Verify Docker Hub is accessible: `docker pull hello-world`
- Check disk space: `df -h`

---

## Running Multiple Machines

If you want to provide compute from multiple machines, **each machine must use a different keypair**. Running the same keypair on multiple machines simultaneously will cause issues.

NousNet uses a delegation system for this:

1. Your main keypair (the one authorized by the run admin) acts as your **master key**
2. You generate additional **delegate keys** for each machine
3. You register those delegates under your master key
4. Each machine uses its own delegate key

### Setup for Multiple Machines

**On your first machine** (where your master key is):

1. Generate a delegate keypair for each additional machine:

```bash
solana-keygen new --outfile ~/.config/solana/psyche-delegate-1.json
solana-keygen new --outfile ~/.config/solana/psyche-delegate-2.json
# ... etc
```

2. Get the public keys:

```bash
solana-keygen pubkey ~/.config/solana/psyche-delegate-1.json
solana-keygen pubkey ~/.config/solana/psyche-delegate-2.json
```

3. Register the delegates under your master key (requires the run admin's join authority pubkey):

```bash
run-manager join-authorization-delegate \
    --rpc [RPC] \
    --wallet-private-key-path [USER_MASTER_KEYPAIR_FILE] \
    --join-authority [JOIN_AUTHORITY_PUBKEY]
    --delegates-clear [true/false] # Optionally remove previously set delegates
    --delegates-added [USER_DELEGATES_PUBKEYS] # Multiple pubkeys can be added
```

> **Note:** Ask the run administrator for the `JOIN_AUTHORITY_PUBKEY`.

4. Copy each delegate keypair file to its respective machine.

5. Fund each delegate wallet with SOL for transaction fees.

**On each additional machine:**

Configure the `.env` file to use that machine's delegate keypair:

```bash
WALLET_PRIVATE_KEY_PATH=/path/to/psyche-delegate-N.json
AUTHORIZER=YOUR_MASTER_PUBLIC_KEY
```

The `AUTHORIZER` should be your master key's public key (the one authorized by the run admin), not the delegate's public key.

---

## Claiming

- **Claiming Rewards:** After participating in training, you can claim rewards using:
  ```bash
  ./run-manager treasurer-claim-rewards \
      --rpc https://api.devnet.solana.com \
      --run-id YOUR_RUN_ID \
      --wallet-private-key-path ~/.config/solana/psyche-node.json
  ```

---

## Quick Reference

| Command                                                       | Purpose                     |
| ------------------------------------------------------------- | --------------------------- |
| `nvidia-smi`                                                  | Verify GPU and drivers      |
| `docker run --rm --gpus all nvidia/cuda:12.0-base nvidia-smi` | Verify GPU access in Docker |
| `solana-keygen pubkey ~/.config/solana/psyche-node.json`      | Get your public key         |
| `solana balance ~/.config/solana/psyche-node.json`            | Check wallet balance        |
| `./run-manager --env-file ~/.config/psyche/run.env`           | Start providing compute     |
| `Ctrl+C`                                                      | Stop the client gracefully  |
