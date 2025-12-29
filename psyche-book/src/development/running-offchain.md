# Running Psyche offchain

When developing for Psyche, you might not want to spin up all the Solana infrastructure if you're working on a feature like the distributed networking or the training code.

To that end, we maintain a "centralized" client & server package that simply communicate over TCP instead of dealing with code deployed to a Solana network.

There's a `server` package, and a `client` package.
To develop with them, you'd spin up one `server` with whatever [run config](../enduser/run-config.md) you want

## Local Testnet

The local testnet is a helper application designed to easily spin up a Server and multiple clients.
It's useful for doing sample runs on your own hardware, and for development.

### Pre-requisites

Since we want to run many clients and the server we'll need several terminal windows to monitor them. The tool uses [tmux](https://github.com/tmux/tmux/wiki/Installing) to create them.

> If you're using the Nix devShell, tmux is already included.

### Running

Since the local-testnet examples uses a local server to provide the data for the clients to train on, you'll need to download the data first.
The best way to do it is install the HuggingFace CLI tool running `curl -LsSf https://hf.co/cli/install.sh | bash`, once installed just run the following command to get some random data and place it in the correct place for the local server to use it:

```bash
hf download emozilla/fineweb-10bt-tokenized-datatrove-llama2 --repo-type dataset --local-dir ./data/fineweb-10bt
```

A sample invocation that fires up 3 clients to train on a 20m model might look like this:

```bash
just local-testnet \
    --num-clients 3 \
    --config-path ./config/consilience-match-llama2-20m-fineweb-pretrain-dev/
```

This will run a server locally that acts as the coordinator and 3 clients that will connect to the server and start training on the downloaded data. We'll talk about the configuration of the run later on but this example will use the config located at `./config/consilience-match-llama2-20m-fineweb-pretrain-dev/state.toml`, there you can have a glimpse of the configuration options.

There's a _lot_ of options to configure the local testnet. Check em out below to configure runs as you see fit:

<details>
    <summary>Command-line options</summary>
    {{#include ../../generated/cli/psyche-centralized-local-testnet.md}}
</details>

## Server & Client

Both of these applications can be spun up individually at your discretion instead of using the local testnet. We include all their command-line options for your reading pleasure:

<details>
    <summary>Client</summary>
    {{#include ../../generated/cli/psyche-centralized-client.md}}
</details>

<details>
    <summary>Server</summary>
    {{#include ../../generated/cli/psyche-centralized-server.md}}
</details>
