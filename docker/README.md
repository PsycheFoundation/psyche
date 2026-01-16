# Docker Psyche

This folder contains some of the docker related files and scripts, mostly entrypoint scripts for the docker containers.
All the used docker images are created via nix docker-tools and can be found in the `packages.nix` file inside the `nix` directory.

The purpose of using docker is two-fold:

- compartmentalize psyche client to be deployed and used in testing and production environments easily.
- implementing end-to-end tests that are as close as possible to a production environment.

There are three concrete use-cases for the docker containers:

- spawning a whole dockerized network with all the components: Solana validator and various clients. These should
  be done via `nix build ...` and `docker compose`.
- booting a testing client in a Solana localnet: basically to join and train in a run with some local or remote
  `solana-test-validator`. In short, it solves some Solana chores such as generating a key pair and adding funds to it.
- booting a production client: these can be used either on the Solana devnet or mainnet. There is no automatic
  management of Solana keys or funds. This should be set manually by the user.

## Docker images

## Psyche Solana client

The `docker-psyche-solana-client` package works as the dockerfile used to build the image for the client that would be used by
end users, in a production-like environment `psyche-solana-cient binary` already built with nix.
The `train_entrypoint.sh` script runs as the default entrypoint for the container, which is no more than a
call to the `psyche-solana-client` binary to start training, and some logic for restart the client in case it crashes.

## Psyche Solana test client

The `docker-psyche-solana-test-client` package is essentially the same as the `docker-psyche-solana-client`, but adds a thin
layer of Solana dev tools like the Solana CLI to make things a little easier in testing scenarios.
The `client_test_entrypoint.sh` script runs as the default entrypoint, which basically generates a Solana key pair,
deposits some funds in it and then starts training. This is done only for convenience in testing scenarios.

## Psyche Solana test validator

The `docker-psyche-solana-test-validator` is meant to build a Solana validator image which will be only used for testing
when spawning a whole network using `docker compose`.
Its main task is to spawn the `solana-test-validator` and deploy the coordinator Solana program.

---

## Running a localnet using Psyche dockerized test clients

### Starting solana-test-validator and deploying Coordinator

If you want to running a validator in your machine, then you will need to start the `solana-test-validator`
binary and then deploy the coordinator program. If you have started the validator and deployed the Coordinator
in another machine, you can skip to the next section.

The script `deploy-solana-test.sh` can be used to set everything; this will essentially start the Solana test
validator, build and deploy the Coordinator program and create a training run.

For creating a lightweight run, you can use

```bash
just dev setup-solana-localnet-light-test-run
```

When the Solana setup finishes, you will see a log saying

```bash
[+] Testing Solana setup ready, starting Solana logs...
Streaming transaction logs. Confirmed commitment
```

In one of the logs when the Coordinator was deployed, you should see the **Program ID**. Knowing
it is useful to be sure that things are being set correctly.

### Starting N dockerized clients

The first thing you should do now is to create an env file in `config/client/.env`. Here you should set
the environment variables `RPC`, `WS_RPC` and `RUN_ID` accordingly:

- `RPC`: The RPC endpoint of your Solana test validator.
- `WS_RPC`: The websocket endpoint of your Solana test validator.
- `RUN_ID`: The name of the training run created previously. Usually set to "test" as a default.

There is an example env file `config/client/.env.example` that you can use as a template.

> [!NOTE]
> If you want to run the network using a local Solana test validator, then you can just copy the `.env.example`
> file as it is:

```bash
cp config/client/.env.example config/client/.env
```

Once you have your `.env` file set, you can build and spawn the dockerized clients for training.
For a computer with N GPUs, you can spawn up to N clients and each one will use one GPU.

This can be done using:

```bash
just setup_gpu_clients <num_clients>
```

where `<num_clients>` should be replaced with the number of clients you want spawn.

As soon as you run the previous `just` command, you will be prompted with a message saying something like

```bash
"<some_program_id>" is the address of the coordinator program that will be used in the psyche test client binary.
Continue? [y/N]
```

Check that it is the same as the **Program ID** from the deployed Coordinator. If it is not, then you should go
to `architectures/decentralized/solana-coordinator/programs/solana-coordinator/src/lib.rs` and change the program ID
being used in the `declare_id!()` macro to the one of the deployed Coordinator.

Once you accept, the docker images will start building and then containers will be started.

---

## Running a whole network with docker compose (mostly used in the testing framework)

To spawn a whole network with all the services included, you can run

```bash
just setup_test_infra
just run_test_infra <num_clients>
```

where `<num_clients>` is the number of clients you want in the run.

You could then check the logs of each container using `docker logs`.

To stop the docker compose network, run

```bash
just stop_test_infra
```

---

## Running dockerized Psyche client in Solana Devnet/Mainnet

First, make sure that the Coordinator is deployed in the Solana devnet and that you have its address.
It will be useful to know that things are going correctly.

Start building the dockerized psyche client with

```bash
just nix build-docker-solana-client
```

You will be prompted with the Coordinator and Authorizer addresses that will be used to build the client binary. Make sure they
are correct and if it is not, go to `architectures/decentralized/solana-coordinator/programs/solana-coordinator/src/lib.rs` and/or
to `architectures/decentralized/solana-authorizer/programs/solana-authorizer/src/lib.rs` and replace the incorrect address in the `declare_id!` macro.
Then try building again with the same command.

Once the docker image is built, the next step is to create a training run. If the run was already created,
go directly to the `Join training run with the dockerized Psyche client` step.

### Creating a run in Devnet

To create a run, you will need to specify the model configuration file and the wallet that will be used
to pay for the creation of the run, as well as the devnet/mainnet RPC and websocket endpoint, and the **run ID**
of the training run.
Create an environment file in `config/client/.env`, if you don't already have one. There variables that should be present are:

- `RPC`: The url to the Solana RPC endpoint
- `WS_RPC`: The url to the Solana websocket endpoint
- `WALLET_FILE`: The path to your Solana keypair
- `RUN_ID`: A string representing the ID of the run
- `CONFIG_PATH`: The path to the configuration of the model to be trained

You can make a copy of the `config/client/.env.example` and set your variables accordingly.
Once everything is set, to create the run using the dockerized Psyche client, you should run

```bash
./docker/psyche_client_create_run.sh
```

watch the logs to know that everything worked correctly.

### Join training run with the dockerized Psyche client

With the Coordinator deployed and the training run created in Devnet/Mainnet, now you can join the run to start training.
You will need your environment file set in `config/client/.env` if you haven't already done it.
The environment variables that should be set are

- `RPC`: The url to the Solana RPC endpoint
- `WS_RPC`: The url to the Solana websocket endpoint
- `WALLET_FILE`: The path to your Solana keypair, used to pay for all transactions in the training process.
- `RUN_ID`: A string representing the ID of the run to join
- `RAW_WALLET_PRIVATE_KEY`: A string representing the Solana private key in base58 format that will be used to sign transactions in the Psyche client.

Once everything is set, you can join the run and start training with

```bash
just run_docker_client --rm -it --name psyche-client --env-file ./config/client/.env
```

### Starting N dockerized clients with one GPU each

#### Funding of accounts

For running N instances of the Psyche client, you will need to fund N accounts. There is a convenience script you can use to
do that. For using it, you will need some account with necessary funds for all of them.

```bash
./scripts/devnet/fund_accounts.sh <PATH_TO_SOLANA_WALLET> <NUMBER_OF_ACCOUNTS> [OPTIONAL]<PATH_TO_KEYS_FILE>
```

The script receives 2 required arguments and 1 optional argument:

- `<PATH_TO_SOLANA_WALLET>`: The path to your Solana keypair, which will be used to fund all the other accounts
- `<NUMBER_OF_ACCOUNTS>`: How many accounts you will want to be funded
- `[OPTIONAL]<PATH_TO_KEYS_FILE>`: If you already have a file with all the account pubkeys, you can provide it for funding those

This script will create the accounts if you don't provide them in a file.
When the script ends, all the accounts will be funded with 1 SOL. The folder with the information about the accounts
will be created in `./devnet_funded_accounts`.

#### Running Psyche clients

Similar to previous sections in this document, you will need your environment file in `config/client/.env`.
The variables that should be set are:

- `RPC`: The url to the Solana RPC endpoint
- `WS_RPC`: The url to the Solana websocket endpoint
- `RUN_ID`: A string representing the ID of the run to join

Once set, you can execute the following script for starting all the clients,

```bash
./scripts/devnet/train-multiple-gpu-devnet.sh <NUM_CLIENTS>
```

where `<NUM_CLIENTS>` is the number of clients you want to run and each one will use one GPU.
The script will automatically use the Solana accounts in the `devnet_funded_accounts` folder for starting the clients.

You can then check the clients logs using the `docker logs` command, each client will be spawned with the
`psyche-client-i` name, where `i` goes from 1 to `N`.
