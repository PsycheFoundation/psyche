# Creating a run

To create a new training run and make it available for nodes to join, you'll need to create it, configure it, and unpause it. By default every new run stays in the pause state until being unpaused by the owner and can be paused anytime.

## Setting up the Run

First, create the run on-chain.
You'll need to provide:

- The RPC & websocket RPC urls so the client can communicate with an RPC node.
- a unique run ID - just a few characters to uniquely identify your run.
- a name & description for your run

Also, for all the commands you will need to provide the path to you Solana private key.

### Setting up Join Authorizations

Before we can get started we need to decide who will be able to join the run.
You can read more about [authorization here](./authentication.md).

We'll need a key-pair file that manages join permissions, it can be the default created by Solana when you do `solana-keygen new` located in `~/.config/solana/id.json`

#### Join Authority for Public Runs

If we're looking to make a permissionless run (anyone can join), we'll need to create an authorization that's valid for everyone.

Running:

```sh
just run_authorizer
```

By default the command will use the values needed to create an authorizer in a Solana localnet using the default Solana key-pair mentioned above and with permissionless access. Basically everyone can join the run without restrictions.

There's three variables that this command can receive:

- `rpc`: The RPC URL to use for the Solana network. By default: `http://127.0.0.1:8899`
- `grantor`: The path to the file with a Solana Keypair, will be used to create the authorization and grant access to the run. By default: `~/.config/solana/id.json`
- `grantee`: The public key of the user that will be granted access to the run. By default: `11111111111111111111111111111111` that means is permissionless.

You can override any of these values like this:

```sh
just rpc=<value> grantor=<value> grantee=<value> run_authorizer
```

#### Join Authority for Private Runs

If we'll only allow some users to join the run we'll need to create one authorization per user (each user can then set multiple delegate keys later) For example to use it locally we can do

```sh
just run_authorizer rpc=<RPC> grantee=<GRANTOR>
```

### Creating the run

> For all the following commands you can use the psyche client with the docker image or directly cloning the Psyche repo and running the package there using `cargo run --bin psyche-solana-client -- ...`.

The run creation will accept a variety of different parameters we'll go through the fundamentals and then we'll go through the rest of the options. Primarily a run needs the RPC of Solana depending on the validator we want to use, a unique identifier known as the `run id`, a join authority that will be the public key that will manage the access to the run (by default will be the one that creates the run) and the private key of the wallet that will be used to create the run.

For a standard run without token incentive distribution layer (see [rewards](../explain/rewards.md) for more details)

```bash
psyche-solana-client create-run \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --description "A description of your run" \
    --join-authority [JOIN_AUTHORITY_PUBKEY] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

For a run that distributes tokens as reward to the training participants, we need to specify the pubkey of the created token in the Solana Blockchain, this will be used as the mint of the collateral token to be distributed:

```bash
psyche-solana-client create-run \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --description "A description of your run" \
    --join-authority [JOIN_AUTHORITY_PUBKEY] \
    --treasurer-collateral-mint [COLLATERAL_MINT_PUBKEY] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

At that point we successfully created our run.

### Initializing configuration

At first the run will not hold any configuration on its behavior and will be paused so no client can join yet.

To set the run's config.
You'll need to provide mostly the same parameters as when creating the run and also the path to a `config.toml` file, that follows the [run config schema](./run-config.md).

```bash
psyche-solana-client update-config \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --config-path [CONFIG_FILE_PATH] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

### Unpausing the run

At this point, your run is ready to go! You can now set its state to "unpaused", and let clients join & begin training your model.

```bash
psyche-solana-client set-paused \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --resume \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

Congratulations! As soon as your first client joins, your model will start being trained.

## Configuring training rewards

If you created a run with rewards, you can configure how many points does each client earns and loses for each epoch of training.

```bash
psyche-solana-client set-future-epoch-rates \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --earning-rate [EARNING_RATE] \
    --slashing-rate [SLASHING_RATE] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

To distribute collateral to users, we need to periodically top-up the run's treasury so that points earned by users during compute can then be claimed against the treasury.

```sh
psyche-solana-client treasurer-top-up-rewards \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --collateral-amount [COLLATERAL_AMOUNT] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

## Getting information about a run

Optionally you can get detailed technical information about a run that was previously created for troubleshooting purposes.

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
