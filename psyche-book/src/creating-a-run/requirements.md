# Requirements

Before creating a training run, ensure you have the following:

## Solana Wallet

You'll need a Solana wallet with:

- A private key file (JSON format) that will serve as the run's main authority
- Sufficient SOL funds to cover transaction fees for creating and managing the run

You can generate a new keypair using:

```bash
solana-keygen new -o my-run-authority.json
```

## Solana RPC Provider

You'll need access to a Solana RPC endpoint to interact with the blockchain. We recommend using a dedicated RPC service such as [Helius](https://www.helius.dev/), [QuickNode](https://www.quicknode.com/), or [Triton](https://triton.one/).

Both HTTP RPC URL and WebSocket RPC URL are required.

## Run Configuration

You'll need to prepare a run configuration file (`config.toml`) that specifies:

- Model architecture and parameters
- Training hyperparameters
- Timing settings (warmup, cooldown, round duration)
- Client requirements (minimum clients, witness nodes)

See [Configuration In Depth](./configuration.md) for the full schema and examples.

## Join Authorization Setup

Decide whether your run will be:

- **Permissionless**: Anyone can join (requires creating a universal authorization)
- **Permissioned**: Only approved keys can join (requires creating individual authorizations)

You'll need to create a separate keypair to serve as the join authority. See [Authentication](./authentication.md) for details.

## Optional: Treasury Setup

If you plan to distribute token rewards to participants, you'll need:

- A token mint address for the collateral token
- Funds to top up the treasury periodically

See [Rewards](../explain/rewards.md) for more information on the reward system.
