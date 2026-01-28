# Requirements

## Pre-requisites

The Psyche client currently only runs under Linux.

## NVIDIA Driver

Psyche requires an NVIDIA CUDA-capable GPU.
If your system does not have NVIDIA drivers installed, follow NVIDIA's [installation guide](https://docs.nvidia.com/datacenter/tesla/driver-installation-guide/) for your Linux distribution.

## Running under Docker

The Psyche client is distributed as a Docker image.
In order to run it, you will need to have some container engine. We develop & test Psyche using Docker, so we recommend you use the same.

If you don't have Docker installed, follow the [Docker Engine installation guide](https://docs.docker.com/engine/install/) for your Linux distribution.

### NVIDIA Container Toolkit

The NVIDIA Container Toolkit is used to enable GPU access inside Docker container, which Psyche uses for model training. To install it, follow the [Nvidia Container Toolkit installation guide](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) for your Linux distribution.

## Solana RPC providers

To ensure reliability, performance, and security, all end-users must configure their own private Solana RPC provider, though configuring two is recommended to accommodate outages and network blips.
We recommend using a dedicated RPC service such as [Helius](https://www.helius.dev/), [QuickNode](https://www.quicknode.com/), [Triton](https://triton.one/), or self-hosting your own Solana RPC node.

## Hardware Requirements

The specific hardware requirements depend on the model being trained and the run configuration. Below are general guidelines.

### GPU (Required)

**Minimum:**

- NVIDIA GPU with CUDA support
- 8GB VRAM for small models

**Recommended by Model Size:**

| Model Size              | VRAM Required      | Example GPUs                 |
| ----------------------- | ------------------ | ---------------------------- |
| Tiny (10M-50M params)   | 4-8GB              | GTX 1070, RTX 3060           |
| Small (50M-500M params) | 8-16GB             | RTX 3080, RTX 4070           |
| Medium (500M-3B params) | 16-24GB            | RTX 3090, RTX 4090, A5000    |
| Large (3B-7B params)    | 24-40GB            | A6000, A100 40GB             |
| Very Large (7B+ params) | 40GB+ or Multi-GPU | A100 80GB, H100, 2x RTX 4090 |

**Notes:**

- VRAM requirements vary with `MICRO_BATCH_SIZE` and parallelism settings
- Larger batch sizes = more VRAM needed but faster training
- `TENSOR_PARALLELISM` can split models across multiple GPUs

**Multi-GPU Configurations:**

- Use `DATA_PARALLELISM` to split data across GPUs (increases throughput)
- Use `TENSOR_PARALLELISM` to split model across GPUs (enables larger models)
- Example: 2x RTX 4090 (24GB each) can train models requiring up to 48GB

### CPU

**Minimum:**

- 4 cores
- 2.0 GHz or faster

**Recommended:**

- 8+ cores for better data loading performance
- Modern CPU (Intel 10th gen+, AMD Ryzen 3000+)

**Considerations:**

- Data preprocessing happens on CPU
- More cores = faster data pipeline
- CPU usage is moderate but consistent

### RAM (System Memory)

**Minimum:** 16GB

**Recommended:**

- 32GB for most models
- 64GB+ for very large models or multi-GPU setups

**Notes:**

- Used for data buffering and model loading
- More RAM allows larger data caches
- Docker container uses host RAM

### Storage

**Minimum:** 50GB free space

**Recommended by Model Size:**

| Model Size            | Storage Needed | Notes                            |
| --------------------- | -------------- | -------------------------------- |
| Small (<1B params)    | 50-100GB       | Model + checkpoints + data cache |
| Medium (1B-7B params) | 100-200GB      | Larger checkpoints               |
| Large (7B+ params)    | 200GB+         | Multiple checkpoint versions     |

**Storage Type:**

- **SSD strongly recommended** for checkpoint loading/saving
- HDD will work but significantly slower
- NVMe SSD ideal for best performance

**What uses storage:**

- Docker images: ~5-10GB
- Model checkpoints: 1-50GB depending on model size
- Training data cache: 10-100GB depending on dataset
- Logs and temporary files: 1-5GB

### Network

**Minimum:** 10 Mbps down, 5 Mbps up

**Recommended:**

- 50+ Mbps down, 20+ Mbps up for smooth P2P model sharing
- 100+ Mbps for optimal performance with multiple clients

**Bandwidth Usage:**

- **Model sharing (P2P):** Varies by model size and frequency
  - Small models: 1-5 GB per epoch
  - Large models: 10-50 GB per epoch
- **Data fetching (HTTP/GCS):** Continuous but moderate
  - Depends on batch size and data provider
- **Blockchain RPC:** Minimal (~1-10 MB/hour)

**Latency:**

- Lower latency = better P2P coordination
- < 100ms to other clients recommended
- < 50ms to Solana RPC ideal

**Firewall/NAT:**

- Outbound connections required (all ports)
- Inbound P2P connections helpful but not required (UPnP can help)
- Use `--network "host"` Docker mode for best P2P connectivity

### Internet Stability

**Critical:** Stable, reliable internet connection

**Requirements:**

- Consistent uptime during training epochs (typically 10-60 minutes)
- No frequent disconnections
- Stable latency

**Why it matters:**

- Disconnections during training = ejection from epoch
- Lost epoch = no rewards for that epoch
- Unstable connection = health check failures

## System Requirements Summary

**Minimal viable setup (small models only):**

- Linux OS
- NVIDIA GPU with 8GB VRAM
- 4-core CPU
- 16GB RAM
- 50GB SSD
- 10 Mbps internet

**Recommended for most models:**

- Linux OS (Ubuntu 20.04+ or similar)
- NVIDIA RTX 3090/4090 or better (24GB VRAM)
- 8-core modern CPU
- 32GB RAM
- 200GB NVMe SSD
- 50+ Mbps stable internet connection
- Quality RPC provider

**Production/Large models:**

- Multi-GPU setup (A100, H100, or multiple RTX 4090s)
- 16+ core CPU
- 64GB+ RAM
- 500GB+ NVMe SSD
- 100+ Mbps dedicated connection
- Redundant RPC providers

## Checking Your System

Before starting, verify your system meets requirements:

**Check GPU:**

```bash
nvidia-smi
```

Look for GPU name and VRAM amount.

**Check available storage:**

```bash
df -h
```

**Check internet speed:**
Use online speed test or:

```bash
curl -s https://raw.githubusercontent.com/sivel/speedtest-cli/master/speedtest.py | python3 -
```

**Check Docker:**

```bash
docker --version
docker run --rm --gpus all nvidia/cuda:11.8.0-base-ubuntu22.04 nvidia-smi
```

If all checks pass, you're ready to proceed to the [Quickstart](./quickstart.md)!
