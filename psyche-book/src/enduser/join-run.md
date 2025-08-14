# Joining a training run

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

## Configuration

A `.env` file should be created containing all the necessary configuration variables for joining a training run. These variables will be used to interact with the Solana blockchain, specify the model you'll contribute compute to, and configure the Psyche client based on your hardware resources.

Your `.env` file should contain at least these configuration options:

- `RPC`: The RPC url of your primary Solana provider.
- `WS_RPC`: The websocket RPC url of the same primary Solana provider.
- `RPC_2`: The RPC url of your other Solana provider. If you don't have one, use a public alternative. For example, <https://api.devnet.solana.com> for Devnet.
- `WS_RPC_2`: The websocket RPC url of your other Solana provider, or a public alternative if you don't have one. For example, wss://api.devnet.solana.com for Devnet.
- `RUN_ID`: The ID of the training run you will join.
- `NVIDIA_DRIVER_CAPABILITIES`: An environment variable that the NVIDIA Container Toolkit uses to determine which compute capabilities should be provided to your container. It is recommended to set it to 'all', e.g. `NVIDIA_DRIVER_CAPABILITIES=all`.
- `DATA_PARALLELISM`: The number of GPUs the training data will be distributed across. This speeds up computation if you have the resources.
- `TENSOR_PARALLELISM`: The number of GPUs the loaded model will be distributed across. This lets you train a model you can't fit on one single GPU.
- `MICRO_BATCH_SIZE`: Number of samples processed per GPU per step (affects memory usage, set it as high as VRAM allows)
- `AUTHORIZER`: The Solana address that delegated authorization to the Solana public key you will use to join the run. You can read more about [authorization here](./authentication.md).

### Testing authorization

You can check if your client is authorized to join a given run by running `psyche-solana-client can-join --run-id <RUN_ID> --authorizer <AUTHORIZER> --pubkey <PUBKEY>`, where `<AUTHORIZER>` is the same Solana authorizer delegate you have configured in your `.env` file, and `<PUBKEY>` is the key associated with the given client's private key. You can also pass the private key via `RAW_WALLET_PRIVATE_KEY="$(cat <path_to_solana_pubkey>)"` instead of passing `--pubkey`.

This command will return successfully if the wallet is authorized to join that run, given the associated authorizer.

### Running the Psyche client docker image

To download and run the Psyche client thru Docker, run the following command, replacing `<path_to_env_file>` and
`<path_to_solana_pubkey>` with your own.

```bash
docker run -d \
    --env-file <path_to_env_file> \
    -e RAW_WALLET_PRIVATE_KEY="$(cat <path_to_solana_pubkey>)" \
    --gpus all \
    --network "host" \
    nousresearch/psyche-client:latest
```

### Fetching earned rewards

If you have accumulated training, you can claim those using the following command

```bash
psyche-solana-client treasurer-claim-rewards \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```
