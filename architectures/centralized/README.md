# Psyche Centralized Server and Client

## Local Testing

You can use the `psyche-centralized-local-testnet` binary in
`/architectures/centralized/local-testnet`, which automates the process of
launching a centralized server and multiple clients using tmux.

### Prerequisites

`nix develop` OR

- tmux
- nvtop (for GPU monitoring)

### Usage

```
cargo run -p psyche-centralized-local-testnet -- --help
```

### Example Invocations

#### Demo

```bash
cargo run -p psyche-centralized-local-testnet start --num-clients 3 --config-path ../../config/llama2-20m-dolma-noverify-no-checkpointer --write-distro-data ./distro-data/llama2-20m-noverify --tui false
```

This command launches a server and 3 clients, using the configuration in
`/path/to/config`, writing gradient data, and disabling the TUI for clients.

#### Testing against node crashes

```bash
just psyche-centralized-local-testnet --num-clients 3 --config-path ./config/kill-test-short-epoch-checkpoint/ --random-kill-num 1 --allowed-to-kill 2,3 --first-client-checkpoint bug-free-chainsaw/tiny-local-20m --hf-token xxxxxxxxxxxxx --write-log
```

This command launches a server with 3 clients, using the config
"kill-test-short-epoch-checkpoint". It randomly kills either client 2 or 3 every
120 seconds (the default interval). Client 1 is doing checkpointing, so we don't
kill it. Client 1 is set to checkpoint to the HF repo
`bug-free-chainsaw/tiny-local-20m`, and we pass an HF token for auth. We also
enable logging to disk.
