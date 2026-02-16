# Quickstart: Join a Run

This guide walks you through joining an existing NousNet (Psyche) training run as a client. It assumes you have been provided the `run-manager` binary and a Run ID (and authorization, if the run is permissioned).

## Prerequisites Checklist

Before starting, ensure you have:

- [ ] Linux operating system (Ubuntu recommended)
- [ ] NVIDIA GPU with sufficient VRAM for the model being trained
- [ ] The `run-manager` binary
- [ ] Run ID from the run administrator
- [ ] Authorization to join the run (if it is permissioned — see [Authentication](./authentication.md))

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

For more details, see the [Docker post-installation guide](https://docs.docker.com/engine/install/linux-postinstall/).

---

## Step 3: Install NVIDIA Container Toolkit

The NVIDIA Container Toolkit enables GPU access inside Docker containers. Follow the [NVIDIA Container Toolkit installation guide](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) for your distribution.

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

After installation, add Solana to your PATH (follow the instructions printed by the installer, or add to `~/.bashrc` / `~/.zshrc`):

```bash
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
```

Then reload your shell and verify:

```bash
source ~/.bashrc   # or source ~/.zshrc
solana --version
```

For more details, see the [Solana installation docs](https://solana.com/docs/intro/installation).

### Generate a Keypair

Create a new Solana keypair for your client:

```bash
solana-keygen new --outfile ~/.config/solana/id.json
```

You'll be prompted to set an optional passphrase. **Back up this keypair file securely.**

Get your public key (you may need to share this with the run administrator for authorization):

```bash
solana-keygen pubkey ~/.config/solana/id.json
```

---

## Step 5: Get Authorization (If Required)

If the run is **permissioned**, the run administrator must authorize your wallet before you can join.

1. **Send your public key** (from `solana-keygen pubkey` above) to the run administrator.
2. They will create an authorization for your key.
3. You will need the **authorizer** address (often your own public key, or a master key if you use delegates) for your `.env` file.

If the run is **permissionless**, you can skip this step. See [Authentication](./authentication.md) for details.

---

## Step 6: Fund Your Wallet

Your wallet needs SOL to pay for transaction fees on the Solana network.

Configure Solana CLI for the correct cluster (e.g. devnet or mainnet — ask the run administrator):

```bash
solana config set --url https://api.devnet.solana.com
```

Request an airdrop on devnet (if applicable):

```bash
solana airdrop 2
```

Verify your balance:

```bash
solana balance
```

> **Note:** For mainnet or if airdrop fails, you must obtain SOL through an exchange or faucet. We recommend using a dedicated RPC service such as [Helius](https://www.helius.dev/), [QuickNode](https://www.quicknode.com/), or [Triton](https://triton.one/).

---

## Step 7: Create the Environment File

Create a `.env` file with your configuration. Example:

```bash
# Path to your Solana keypair
WALLET_PRIVATE_KEY_PATH=~/.config/solana/id.json

# Required: Solana RPC endpoints (use the cluster the run uses)
RPC=https://api.devnet.solana.com
WS_RPC=wss://api.devnet.solana.com

# Required: Run ID (from run administrator)
RUN_ID=your_run_id_here

# Required for permissioned runs: authorizer address (from run administrator or your pubkey)
AUTHORIZER=YOUR_AUTHORIZER_PUBKEY_HERE

# Recommended: fallback RPC (for reliability)
RPC_2=https://your-backup-rpc.com
WS_RPC_2=wss://your-backup-rpc.com

# Recommended for GPU access in container
NVIDIA_DRIVER_CAPABILITIES=all
```

**Replace:**

| Variable                    | Replace With                                |
| --------------------------- | ------------------------------------------- |
| `your_run_id_here`          | The run ID from your run administrator      |
| `YOUR_AUTHORIZER_PUBKEY_HERE` | Your authorizer public key (see [Authentication](./authentication.md)) |

Optional variables (defaults are usually fine): `DATA_PARALLELISM`, `TENSOR_PARALLELISM`, `MICRO_BATCH_SIZE`. See [Joining a run](./join-run.md#additional-config-variables) for details.

---

## Step 8: Verify Authorization (Optional)

Before starting, you can confirm your wallet is allowed to join the run. The `can-join` subcommand does not read your `.env` file, so pass `--rpc` (and optionally `--ws-rpc`) or set them in your environment:

```bash
./run-manager can-join \
    --rpc https://api.devnet.solana.com \
    --run-id YOUR_RUN_ID \
    --authorizer YOUR_AUTHORIZER \
    --address YOUR_PUBKEY
```

- Use the same `RUN_ID` and `AUTHORIZER` as in your `.env`, and `YOUR_PUBKEY` from `solana-keygen pubkey ~/.config/solana/id.json`.
- For **permissionless** runs, use `--authorizer 11111111111111111111111111111111`.
- If the command prints `✓ Can join run ...`, you can proceed.

---

## Step 9: Run the Manager

Make the binary executable if needed:

```bash
chmod +x ./run-manager
```

Start the client (using a stable session like `tmux` is recommended for long runs):

```bash
./run-manager --env-file /path/to/your/.env
```

The run-manager will pull the correct Docker image, start the training container, and stream logs. To stop the client, press `Ctrl+C`.

---

## Step 10: Verify It's Working

You should see:

1. Image pull progress (Docker downloading the client image)
2. Container startup and connection to the coordinator
3. Training progress logs

To stop the client gracefully, press `Ctrl+C`. For more details and troubleshooting, see [Joining a run](./join-run.md).

---

## Troubleshooting

### Docker Not Found

**Error:** `Failed to execute docker command. Is Docker installed and accessible?`

**Solution:** Install Docker and add your user to the `docker` group:

```bash
sudo usermod -aG docker $USER
```

Then log out and back in.

### GPU Not Detected in Container

**Error:** Container starts but crashes immediately, or GPU-related errors in logs

**Solution:** Verify drivers and NVIDIA Container Toolkit. Run:

```bash
docker run --rm --gpus all nvidia/cuda:12.2.2-devel-ubuntu22.04
```

### Wallet Not Found

**Error:** `Failed to read wallet file from: ...`

**Solution:** Check that `WALLET_PRIVATE_KEY_PATH` in your `.env` points to an existing file:

```bash
ls -l ~/.config/solana/id.json
```

Use `chmod 600` on the keypair file if needed.

### RPC Connection Failures

**Error:** `RPC error: failed to get account` or connection timeouts

**Solution:** Verify `RPC` and `WS_RPC` in your `.env`. Try backup endpoints (`RPC_2`, `WS_RPC_2`). Ensure your RPC provider and cluster (devnet/mainnet) match the run.

### Not Authorized to Join

**Error:** Authorization or permission errors when joining

**Solution:** Confirm with the run administrator that your public key is authorized. Re-run the check from Step 8 (use the same RPC as in your `.env`):

```bash
./run-manager can-join --rpc YOUR_RPC_URL --run-id YOUR_RUN_ID --authorizer YOUR_AUTHORIZER --address YOUR_PUBKEY
```

### Container Keeps Restarting (Version Mismatch)

**Symptom:** Container restarts repeatedly with "version mismatch"

**Solution:** Usually a Docker image pull issue. Check internet, run `docker pull hello-world`, and disk space: `docker system df`.

### Process Appears Stuck

**Solution:** Press `Ctrl+C` to stop run-manager. If it does not stop, list containers with `docker ps -a` and stop the container manually: `docker stop CONTAINER_ID`.

---

## Claiming Rewards

If the run distributes rewards, after participating you can claim them:

```bash
./run-manager treasurer-claim-rewards \
    --rpc YOUR_RPC_URL \
    --run-id YOUR_RUN_ID \
    --wallet-private-key-path ~/.config/solana/id.json
```

Use the same RPC and Run ID as in your `.env`. See [Joining a run](./join-run.md#claiming-rewards) for details.

---

## Quick Reference

| Command                                                    | Purpose                    |
| ---------------------------------------------------------- | -------------------------- |
| `nvidia-smi`                                               | Verify GPU and drivers     |
| `docker run --rm --gpus all nvidia/cuda:12.0-base nvidia-smi` | Verify GPU access in Docker |
| `solana-keygen pubkey ~/.config/solana/id.json`            | Get your public key        |
| `solana balance`                                           | Check wallet balance       |
| `./run-manager can-join --rpc URL --run-id X --authorizer Y --address Z` | Check if authorized        |
| `./run-manager --env-file /path/to/.env`                   | Start joining the run      |

For the full guide, optional configuration, and building from source, see [Joining a run](./join-run.md).
