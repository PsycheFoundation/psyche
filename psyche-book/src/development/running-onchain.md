# Running Psyche on-chain

To build the Solana programs, you’ll need a handful of Solana tools installed. See [the setup](./setup.md) if you’re not using Nix. If you’re using Nix, make sure you are in the development environment by running `nix develop`.

To start, you’ll need to create a Solana wallet to fund your transactions.

```bash
solana-keygen new
```

By default, the keypair will be generated in `~/.config/solana/id.json`.

## Run on a local validator (localnet)

To quickly test decentralized training, you can spin up a Solana validator locally and fund your Solana wallet with fake tokens to make transactions. To set up a new training run with this tool, in a new terminal run the following command:

```bash
just dev setup-solana-localnet-test-run run_id=<RUN_ID>
```

This will:

- Set up a `solana-test-validator`
- Deploy all the required programs (Coordinator and Authorizer)
- Create a local run with the name `<RUN_ID>`. If no run name is provided, the name `test` will be used by default. The run ID should not exceed 32 characters; it will be truncated if it exceeds this limit.

Then, in another terminal, run a client to train the test model and join the run with the name `<RUN_ID>`.

```bash
just dev start-training-localnet-client run_id=<RUN_ID>
```

This will start a run to train a 1.1B parameter model with all the parallelism features enabled. This Psyche client will use a temporary private key, which will be generated and deleted automatically when running the command above. If you want to inspect these keys, they will be stored in `~/.config/solana/solana-keys`. To run it with a specific private key, you can run the same command while adding the `WALLET_FILE` environment variable:

```bash
WALLET_FILE=/path/to/wallet.json just dev start-training-localnet-client run_id=<RUN_ID>
```

For a more lightweight run to avoid OOM errors, or just to use less of your hardware (we see you, 8 GB VRAM cards!), there’s also:

```bash
just dev setup-solana-localnet-light-test-run
just dev start-training-localnet-light-client
```

This will train a 12M parameter model, which should fit on most GPUs.

To spin up another client and join the run, you can run the same command as before:

```bash
just dev start-training-localnet-client run_id=<RUN_ID>
```

or

```bash
just dev start-training-localnet-light-client run_id=<RUN_ID>
```

This will create a new temporary Solana keypair in `~/.config/solana/solana-keys`, which will be removed when the client is stopped, so you can spawn as many clients as you want.

## Run on Solana Devnet

You’ll need to fund your wallet to make transactions on Devnet. You can [request an airdrop](https://faucet.solana.com/) from the Solana Foundation of up to 10 devnet SOL every 8 hours. To get your public key, run:

```bash
solana-keygen pubkey <PATH_TO_KEYPAIR>
```

If no path to a keypair is provided, it will use the default keypair located at `~/.config/solana/id.json`. Paste the resulting key into the airdrop website to receive tokens.

You can then follow the same steps for deploying the programs, creating a run, and training as on localnet, but using the following `just` commands:

```bash
just dev setup-solana-devnet-test-run
just dev start-training-devnet-client
```

along with the `-light` variants:

```bash
just dev setup-solana-devnet-light-test-run
just dev start-training-devnet-light-client
```

Remember to set the `WALLET_FILE` environment variable to the path of your Solana keypair file when running the training commands, since this will be the wallet holding the devnet funds.

These commands work almost the same as the localnet ones, but they use the public Solana Devnet RPC endpoint (`https://api.devnet.solana.com`). Also, for all programs (Coordinator, Authorizer, and Treasurer), we need to generate new program IDs—basically the “addresses” where the contracts will be deployed—since the current IDs are used by the Psyche team for development and can’t be overridden. More details on how to update program IDs can be found in the [changing contracts section](#changing-contracts).

## Creating a permissioned run

All the commands and setups above use a permissionless run by default. In most testing cases, this is fine, since you can join any number of clients without restrictions. If a permissioned run is needed, you’ll have to take a few extra steps.

You have the same variants as before, but with the permissioned option enabled. These commands won’t create the permissionless authorization and will instead allow you to create the required authorizations manually. The commands are:

```bash
# Localnet
just dev setup-solana-localnet-permissioned-test-run
just dev setup-solana-localnet-permissioned-light-test-run
just dev setup-solana-localnet-permissioned-test-run-treasurer
just dev setup-solana-localnet-permissioned-light-test-run-treasurer

# Devnet
just dev setup-solana-devnet-permissioned-test-run
just dev setup-solana-devnet-permissioned-light-test-run
just dev setup-solana-devnet-permissioned-test-run-treasurer
just dev setup-solana-devnet-permissioned-light-test-run-treasurer
```

depending on your needs.

You can then create an authorization manually by specifying who grants the authorization (the run owner) and who receives it. Run:

```sh
cargo run --release --bin psyche-solana-client -- \
    join-authorization-create \
    --rpc [RPC] \
    --wallet-private-key-path [JOIN_AUTHORITY_KEYPAIR_FILE] \
    --authorizer [USER_MASTER_PUBKEY]
```

Here, the `--wallet-private-key-path` is the path to the Solana KeyPair that will handle authorization to join and the `--authorizer` is the pubkey of the account that will receive the authorization. To get the pubkey of a KeyPair file you can use the `solana-keygen pubkey <FILE>` command.

You can then join any authorized client by running the training commands described above, adding the authorized key as an environment variable, for example:

```sh
AUTHORIZER=<GRANTEE_PUBKEY> just dev start-training-localnet-light-client
```

## Running a run with rewards

There’s another program that adds a new layer to the Psyche run called the `Treasurer`. When this program is deployed, it adds a rewards layer on top of the Coordinator, calculating how much of a specific token each client receives for their training time. This contract isn’t required to test a run, but it adds reward functionality if you want to test it. You can find a more in-depth explanation in the [rewards section](../explain/rewards.md).

To test this, all the commands mentioned above also have variants that include the Treasurer, such as:

```bash
# Localnet
just dev setup-solana-localnet-test-run-treasurer
just dev setup-solana-localnet-light-test-run-treasurer

# Devnet
just dev setup-solana-devnet-test-run-treasurer
just dev setup-solana-devnet-light-test-run-treasurer
```

These commands deploy the Treasurer alongside the other contracts, create a new test token using the [SPL Token tool](https://solana.com/docs/tokens/basics) on the selected network, and top up the run with rewards and collateral for clients that train for more than one epoch.

All these commands also have permissioned variants.

### Recovering dev tokens

Most devnet tokens are used to deploy the various contracts. You can reclaim these tokens once you’ve finished testing, which is useful since the Solana devnet faucet is limited. Run:

```bash
just dev close-dev-programs
```

This will close all deployed accounts on devnet and return the tokens to the wallet used for deployment. Be aware that this is an irreversible action: once a program is closed, you can’t reuse the same program ID and must generate a new one.

## Psyche decentralized client reference

All the commands above use the same `psyche-solana-client` package with specific parameters for quick local testing, but it supports many different configurations to test various scenarios.

Here’s a summary of the available commands and options:

<details>
    <summary>Command-line options</summary>
    {{#include ../../generated/cli/psyche-solana-client.md}}
</details>

## Changing contracts

Psyche uses two main accounts deployed to Solana—the Coordinator and the Authorizer—and one optional account, the Treasurer. If you’re developing changes that modify the on-chain account layout, deploying an updated Coordinator program will likely break existing runs that already have coordinator accounts instantiated.

Because of this, changes to on-chain data structures require deploying a new Coordinator program under a new Program ID to avoid breaking existing runs.

To do this yourself, you’ll need to generate a new Program ID (and keypair).

To deploy a program to devnet or localnet with a new program keypair, regenerate its devnet/localnet keypair file (which is checked into the repo).

For the Solana Coordinator, run:

```bash
solana-keygen new -o architectures/decentralized/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json -f
```

You can view the newly generated program ID with:

```bash
solana-keygen pubkey architectures/decentralized/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json
```

Make sure to update the `declare_id` value with the new key before deploying the updated contracts, either manually or using `anchor keys sync` in the appropriate project folder.

If you want to push these changes to the repo, you’ll need to use `git add -f`, since these files are normally `.gitignore`d.
