# Cluster Observer

The `observer` is a terminal UI that can replay events from the `psyche-event-sourcing` crate.
It's designed to replay a stream of events from nodes so you can see when & how things go wrong.

## Running

```bash
nix run .#observer
```

or

```bash
cargo run --bin observer
```

## Testing on local runs

### Centralized local testnet (`just local-testnet`)

`just local-testnet` passes `--events-dir ./events/local-testnet` to every client it spawns, so events are waiting for you as soon as training starts.

```bash
# start a small 2-client run
just local-testnet --num-clients 2 --config-path ./config/consilience-match-llama2-20m-fineweb-pretrain-dev/

# in a second terminal
nix run .#observer -- ./events/local-testnet
```

Events persist in `events/local-testnet/` after the testnet exits.

## Arbitrary events files

Any Psyche client supports `--events-dir` (or the `EVENTS_DIR` env var):

```bash
psyche-solana-client train \
    --events-dir /tmp/run-events \
    --run-id my-run \
    # ... etc

# in a second terminal:
nix run .#observer -- --events-dir /tmp/run-events
```
