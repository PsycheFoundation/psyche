# Creating a run

To create a new training run and make it available for nodes to join, you'll need to create it, configure it, and unpause it. By default every new run stays in the pause state until being unpaused by the owner and can be paused anytime.

## Creating the Coordinator account

First, create the run on-chain.
You'll need to provide:

- The RPC & websocket RPC urls so the client can communicate with an RPC node.
- a unique run ID - just a few characters to uniquely identify your run.
- a name & description for your run

Also, for all the commands you will need to provide the path to you Solana private key.

### Setup Joining Authorizations

Before we can get started we need to decide who will be able to join the run.
You can read more about [authorization here](./authentication.md).

We'll need a key-pair file that manages join permissions, it can be the default created by Solana when you do `solana-keygen new` located in `~/.config/solana/id.json`

#### Join Authority for Public Runs

If we're looking to make a permissionless run (anyone can join), we'll need to create an authorization that's valid for everyone.

Running:

```sh
just run_authorizer rpc=<RPC> grantor=<GRANTOR> grantee=<GRANTOR>
```

By default the `just run_authorizer` command will use the values needed to create an authorizer in a solana local validator using the default Solana key-pair mentioned above and with permissionless access. Basically everyone can join the run without restrictions.

#### Join Authority for Private Runs

If we'll only allow some users to join the run we'll need to create one authorization per user (each user can then set multiple delegate keys later) For example to use it locally we can do

```sh
just run_authorizer rpc=<RPC> grantee=<GRANTOR>
```

### Creating a run without token rewards

For a standard run without token incentive distribution layer

```bash
psyche-solana-client create-run \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --join-authority [JOIN_AUTHORITY_PUBKEY] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

### Creating a run with token rewards

For a run that distributes tokens as reward to the training participants, we need to specify the mint of the collateral token to be distributed:

```bash
psyche-solana-client create-run \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --join-authority [JOIN_AUTHORITY_PUBKEY] \
    --treasurer-collateral-mint [COLLATERAL_MINT_PUBKEY] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
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
    --run-id [RUN_ID] \
    --config-path [CONFIG_FILE_PATH] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

## Starting the training

At this point, your run is ready to go! You can now set its state to "unpaused", and let clients join & begin training your model.

```bash
psyche-solana-client set-paused \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --resume \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

Congratulations! As soon as your first client joins, your model is being trained.

## Configuring training rewards

You can configure how many points does each client earns and loses for each epoch of training.

```bash
psyche-solana-client set-future-epoch-rates \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --earning-rate [EARNING_RATE] \
    --slashing-rate [SLASHING_RATE] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

## Funding the run with collateral

To distribute collateral to users, we need to periodically top-up the run's treasury so that points earned by users during compute can then be claimed against the treasury.

```sh
psyche-solana-client treasurer-top-up-rewards \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --collateral-amount [COLLATERAL_AMOUNT] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

## Inspect the content of a run

Optionally you can get detailled technical information about a run that was previously created for troubleshooting purposes.

```bash
psyche-solana-client json-dump-run \
    --rpc [RPC] \
    --run-id [RUN_ID]
```

For more info about a specific user inside of a run, you can also use:

```bash
psyche-solana-client json-dump-user \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --wallet [PUBLIC_KEY]
```
