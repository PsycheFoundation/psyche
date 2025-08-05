# Creating a run

To create a new training run and make it available for nodes to join, you'll need to create it, configure it, and unpause it.

## Creating the account

First, create the run on-chain.
You'll need to provide:

- the RPC & websocket RPC urls so the client can communicate with an RPC node.
- a unique run ID - just a few characters to uniquely identify your run.
- a name & description for your run

### Without treasurer

For a standard run without token incentive distribution layer

```bash
psyche-solana-client create-run \
    --rpc [RPC] \
    --ws-rpc [WS_RPC] \
    --run-id [RUN_ID] \
    --name [NAME] \
    --description [DESCRIPTION]
```

### With treasurer

For a run that distributes tokens as reward to the training participants, we need to specify the mint of the token mint to be distributed:

```bash
psyche-solana-client create-run \
    --rpc [RPC] \
    --ws-rpc [WS_RPC] \
    --run-id [RUN_ID] \
    --treasurer-collateral-mint [REWARD_MINT] \
    --name [NAME] \
    --description [DESCRIPTION]
```

## Initializing configuration

Then, set the run's config.
You'll need to provide:

- the RPC & websocket RPC urls so the client can communicate with an RPC node.
- the run ID you previously used
- the path to a `config.toml` file, following the [run config schema](./run-config.md)

```bash
psyche-solana-client update-config \
    --rpc [RPC] \
    --ws-rpc [WS_RPC] \
    --run-id [RUN_ID] \
    --config-path [CONFIG_FILE]
```

## Starting the training

At this point, your run is ready to go! You can now set its state to "unpaused", and let clients join & begin training your model.

```bash
psyche-solana-client set-paused \
    --rpc [RPC] \
    --ws-rpc [WS_RPC] \
    --run-id [RUN_ID] \
    resume
```

Congratulations! As soon as your first client joins, your model is being trained.

## Configuring training rewards

You can configure how many points does each client earns and loses for each epoch of training.

```bash
psyche-solana-client set-future-epoch-rates \
    --rpc [RPC] \
    --ws-rpc [WS_RPC] \
    --run-id [RUN_ID] \
    --earning-rate [EARNING_RATE] \
    --slashing-rate [SLASHING_RATE]
```
