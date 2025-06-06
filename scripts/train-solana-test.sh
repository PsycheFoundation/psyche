#!/usr/bin/env bash

set -euo pipefail

# use the agenix provided wallet if you have it
DEFAULT_WALLET=${devnet__keypair__wallet_PATH:-"$HOME/.config/solana/id.json"}
WALLET_FILE=${WALLET_FILE:-"$DEFAULT_WALLET"}
RPC=${RPC:-"http://127.0.0.1:8899"}
WS_RPC=${WS_RPC:-"ws://127.0.0.1:8900"}
RUN_ID=${RUN_ID:-"test"}

# presets for a DGX or an HGX
DP=${DP:-"8"}
TP=${TP:-"1"}
BATCH_SIZE=${BATCH_SIZE:-"1"}

# fine if this fails
solana airdrop 10 "$(solana-keygen pubkey ${WALLET_FILE})" --url "${RPC}" || true

# export RUST_LOG="info,psyche=debug"
export RUST_LOG="info,psyche=debug,psyche_client=trace"

cargo run --release --bin psyche-solana-client -- \
    train \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --data-parallelism ${DP} \
    --tensor-parallelism ${TP} \
    --micro-batch-size ${BATCH_SIZE} \
    --logs "console" \
    "$@"
