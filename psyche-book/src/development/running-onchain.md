# Running Psyche on-chain

To build the Solana programs, you'll need a handful of Solana tools installed. See [the setup](./setup.md) if you're not using Nix. If you're using Nix, make sure you are in the development environment by running `nix develop`.

To start, you'll need to create a Solana wallet to fund your transactions.

```bash
solana-keygen new
```

By default the KeyPair will be generated in `~/.config/solana/id.json`.

## Run on a local validator (localnet)

To quickly be able to test the decentralized training you can spin up a Solana Validator locally an fund your Solana wallet with fake tokens to be able to make transactions there. To setup a new training run with this tool, in a new terminal, run the following command:

```bash
just dev setup-solana-localnet-test-run run_id=<RUN_ID>
```

This will:

- Setup a `solana-test-validator`
- Deploy all the required programs (Coordinator and Authorizer)
- Create a local run with name `<RUN_ID>`. If no run name is provided, the name `test` will be used by default. The run id should not exceed 32 characters, it will be truncated if it exceeds this limit.

Then, in another terminal, run a client to train the test model and joining the run with name `RUN_ID`.

```bash
just dev start-training-localnet-client run_id=<RUN_ID>
```

This will start a run to train a 1.1b parameter model with all the parallelism features enabled. This Psyche client will use a temporal private key, which will be generated and deleted automatically running the mentioned command. In case you want to check these keys, they will be stored in `~/.config/solana/solana-keys`. To run it with a specific private key, you can run the same command but adding the `WALLET_FILE` env var:

```bash
WALLET_FILE=/path/to/wallet.json just dev start-training-localnet-client run_id=<RUN_ID>
```

For a more lightweight run to avoid OOM errors, or just to use your hardware less, (we see you 8gb VRAM cards!) there's also:

```bash
just dev setup-solana-localnet-light-test-run
just dev start-training-localnet-light-client
```

This will train a 12m which should fit on most GPUs.

To spin up another client and join the run you can run the same command as before:

```bash
just dev start-training-localnet-client run_id=<RUN_ID>
```

or

```bash
just dev start-training-localnet-light-client run_id=<RUN_ID>
```

This will create a new temporal solana keypair in `~/.config/solana/solana-keys` and be removed when the client is stopped so you can spawn as many clients you want.

## Run on Solana's Devnet

You'll need to fund your wallet to make transactions on Devnet.
You can [request an airdrop](https://faucet.solana.com/) from the Solana foundation of up to 10 devnet sol every 8 hours. To get your public key, run:

```bash
solana-keygen pubkey <PATH_TO_KEYPAIR>
```

If no path to keypair is provided, it will use the default keypair located at `~/.config/solana/id.json`. Paste the resulting key into the airdrop website to get tokens.

You can then use the same steps for deploying the programs, creating a run, and training on localnet above, but using the following `just` commands:

```bash
just dev setup-solana-devnet-test-run
just dev start-training-devnet-client
```

alongside the `-light` variants

```bash
just dev setup-solana-devnet-light-test-run
just dev start-training-devnet-light-client
```

Remember to set the `WALLET_FILE` environment variable to the path of your Solana keypair file, since this will be the one with the devnet funds.

These commands work almost the same as the ones using localnet, but they use the public Solana Devnet RPC endpoint (`https://api.devnet.solana.com`). Also, for all the programs (Coordinator, Authorizer, and Treasurer), we need to generate new program IDs, basically the “addresses” where the contracts will be deployed, since the current IDs are the ones used by the Psyche team for development and can’t be overridden. More details on how we update the program ids in the [changing contracts section](#changing-contracts).

## Running run with rewards

There's another program that adds a new layer to the whole Psyche run named the `Treasurer` when this program gets deployed it adds a whole new layer of rewards on top of the Coordinator that will calculate the amount of a specific token that a client will get for their training time. This contract is not mandatory to test a run it will only add the functionality in case we want to test the rewards. You can check a more in depth explanation on the [rewards section](../explain/rewards.md).

To be able to test this, all the commands we already mentioned have also a version with the treasurer usage, like:

```bash
# Localnet
just dev setup-solana-localnet-test-run-treasurer
just dev setup-solana-localnet-light-test-run-treasurer

# Devnet
just dev setup-solana-devnet-test-run-treasurer
just dev setup-solana-devnet-light-test-run-treasurer
```

These commands will deploy the Treasurer along the other contracts and also create a new test token with the [SPL Token tool](https://solana.com/docs/tokens/basics) in the used network to be able to use it as collateral of in the run.

## Psyche decentralized client reference

All the commands above will use the same package `psyche-solana-client` with specific parameters to be able to do a quick train on the local validator but it actually has a _lot_ of different configs to be able to test and run different scenarios.

Here's a summary of all the available commands and options that can be used:

<details>
    <summary>Command-line options</summary>
    {{#include ../../generated/cli/psyche-solana-client.md}}
</details>

## Changing contracts

Psyche uses two main accounts that are deployed to Solana, the coordinator and the authorizer and one optional account that is the Treasurer.
If you're developing things that change the structure of the program's accounts layout, deploying an update to the coordinator program will likely cause breakage with existing runs that have coordinator accounts already instantiated.

Therefore, changes to the data structures that end up on-chain will require a deployment of a new coordinator program under a new ProgramID to prevent breakage of existing runs.

In order to do this by yourself, you'll need to generate a new ProgramID (and keypair).

To deploy a program to devnet or localnet _with a new program keypair_,
regenerate its devnet/localnet keypair file (checked into the repo!)

For the solana coordinator, that would be:

```bash
solana-keygen new -o architectures/decentralized/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json -f
```

You can see the newly generated program ID by running

```bash
solana-keygen pubkey architectures/decentralized/solana-coordinator/target/deploy/psyche_solana_coordinator-keypair.json
```

Make sure to then update the `declare_id`'s content with the new keys before deploying the new development contracts, either manually or with `anchor keys sync` in the appropriate project folder.

if you want to push these changes to the repo, you'll need to use `git add -f`, since they're normally `.gitignore`d.
