# Creating a run

To create a new training run and make it available for nodes to join, you’ll need to create it, configure it, and unpause it. By default, every new run starts in a paused state until it is explicitly unpaused by the owner, and it can be paused again at any time.

## Setting up the Run

First, create the run on-chain.
You’ll need to provide:

- The RPC and WebSocket RPC URLs so the client can communicate with an RPC node.
- A unique run ID — just a few characters to uniquely identify your run.

> For all of the following commands, you can either use the Psyche client Docker image or clone the Psyche repository and run the package directly using
> `cargo run --bin psyche-solana-client -- ...`.

### Setting up Join Authorizations

Before getting started, we need to decide who will be able to join the run.
You can read more about this in the [authorization](./authentication.md) section.

We’ll need a keypair file that manages join permissions. This can be the default one created by Solana when running `solana-keygen new`, located at `~/.config/solana/id.json` or whatever key you want to control the join permissions.

#### Join Authority for Public Runs

If we're looking to make a permissionless run (anyone can join), we'll need to create an authorization that's valid for everyone. In this case, if we set the authorizer to be `11111111111111111111111111111111` in the following command it will be valid for everyone so any other client can join the created run without additional restrictions.

```sh
run-manager join-authorization-create \
    --rpc [RPC] \
    --wallet-private-key-path [SOLANA_KEY_FILE] \
    --authorizer 11111111111111111111111111111111
```

In this case the `SOLANA_KEY_FILE` needs to be the path to a Solana keypair that is creating the authorization for this specific run.

#### Join Authority for Private Runs

If you only want certain users to join a run, you’ll need to create one authorization per user (each user can later set multiple delegate keys).

For example, imagine you have a keypair for the run creator at `~/.config/solana/owner.json`, which is also the account that grants authorization, and another keypair at `~/.config/solana/joiner.json` for the client that is being authorized by the owner and wants to join and train in the run.

First, create the authorization with the following parameters:

```sh
run-manager join-authorization-create \
    --rpc [RPC] \
    --wallet-private-key-path ~/.config/solana/owner.json \
    --authorizer $(solana-keygen pubkey ~/.config/solana/joiner.json)
```

This command uses the public key of the user you want to allow to join and the keypair of the run owner to create the appropriate authorization. The `solana-keygen pubkey` command just gives you the public key derivated from the keypair file.

Now all that’s left is for the joiner to use their public key, now associated with the newly created authorization, when joining the run using the `--authorization` flag in the `train` command. More details can be found in the [joining a run](./join-run.md) section.

### Creating the run

Run creation accepts a variety of parameters. We’ll start with the fundamentals and then cover the remaining options. At a minimum, a run needs:

- The Solana RPC endpoint corresponding to the validator you want to use.
- A unique identifier, known as the **run ID**.
- A join authority, which is the public key that manages access to the run (by default, this is the key that creates the run).
- The private key of the wallet used to create the run.

For a standard run without a token incentive distribution layer (see [rewards](../explain/rewards.md) for more details):

```bash
run-manager create-run \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --join-authority [JOIN_AUTHORITY_PUBKEY] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH] \
    --client-version "latest"
```

For a run that distributes tokens as rewards to training participants, you must specify the public key of the token created on the Solana blockchain. This will be used as the mint for the collateral token to be distributed:

```bash
run-manager create-run \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --join-authority [JOIN_AUTHORITY_PUBKEY] \
    --treasurer-collateral-mint [TOKEN_PUBKEY] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH] \
    --client-version "latest"
```

At this point, your run has been successfully created.

### Initializing configuration

Initially, the run will not have any configuration defined and will remain paused, so no clients can join yet.

To set the run configuration, you’ll need to provide mostly the same parameters as when creating the run, along with the path to a `config.toml` file that follows the [run config schema](./run-config.md).

```bash
run-manager update-config \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --config-path [CONFIG_FILE_PATH] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

### Unpausing the run

At this point, your run is ready to go. You can now set its state to **unpaused**, allowing clients to join and begin training your model.

```bash
run-manager set-paused \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --resume \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

Congratulations! As soon as your first client joins, your model will start training.

## Configuring training rewards

If you created a run with rewards enabled, you can configure how many points each client earns or loses per training epoch.

```bash
run-manager set-future-epoch-rates \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --earning-rate-total-shared [EARNING_RATE] \
    --slashing-rate-per-client [SLASHING_RATE] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

To distribute collateral to users, you must periodically top up the run’s treasury so that points earned during computation can be claimed.

```sh
run-manager treasurer-top-up-rewards \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --collateral-amount [COLLATERAL_AMOUNT] \
    --wallet-private-key-path [JSON_PRIVATE_KEY_PATH]
```

## Getting information about a run

Optionally, you can retrieve detailed technical information about a previously created run for troubleshooting purposes.

```bash
run-manager json-dump-run \
    --rpc [RPC] \
    --run-id [RUN_ID]
```

For more information about a specific user within a run, you can also use:

```bash
run-manager json-dump-user \
    --rpc [RPC] \
    --run-id [RUN_ID] \
    --address [PUBLIC_KEY]
```
