# Quickstart

This guide provides a streamlined walkthrough for creating and launching a training run.

## Step 1: Setup Join Authorization

First, create a keypair that will manage join permissions:

```bash
solana-keygen new -o join_authority.json
```

### For a public run (anyone can join):

```sh
sh scripts/join-authorization-create.sh [RPC] join_authority.json 11111111111111111111111111111111
```

### For a private run (specific users only):

```sh
# Create one authorization per user
sh scripts/join-authorization-create.sh [RPC] join_authority.json [USER_PUBKEY]
```

## Step 2: Create the Run

### Without token rewards:

```bash
psyche-solana-client create-run \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --join-authority [JOIN_AUTHORITY_PUBKEY] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

### With token rewards:

```bash
psyche-solana-client create-run \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --join-authority [JOIN_AUTHORITY_PUBKEY] \
    --treasurer-collateral-mint [COLLATERAL_MINT_PUBKEY] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

## Step 3: Set Run Configuration

Upload your `config.toml` file to configure the run:

```bash
psyche-solana-client update-config \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --config-path [CONFIG_FILE_PATH] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

## Step 4: Start Training

Unpause the run to allow clients to join and begin training:

```bash
psyche-solana-client set-paused \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --resume \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

## Step 5 (Optional): Configure Rewards

Set earning and slashing rates for participants:

```bash
psyche-solana-client set-future-epoch-rates \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --earning-rate [EARNING_RATE] \
    --slashing-rate [SLASHING_RATE] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

## Step 6 (Optional): Fund the Treasury

If using token rewards, top up the treasury:

```sh
psyche-solana-client treasurer-top-up-rewards \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --collateral-amount [COLLATERAL_AMOUNT] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

## Monitoring Your Run

### Inspect run details:

```bash
psyche-solana-client json-dump-run \
    --rpc [RPC] \
    --run-id [RUN_ID]
```

### Check specific user status:

```bash
psyche-solana-client json-dump-user \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --wallet [PUBLIC_KEY]
```

## Next Steps

- Review [Configuration In Depth](./configuration.md) to optimize your run settings
- Understand [Authentication](./authentication.md) for managing client access
- Check the [FAQ](./faq.md) for common questions about running and managing runs
