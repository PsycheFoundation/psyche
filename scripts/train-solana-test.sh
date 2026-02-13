#!/usr/bin/env bash

set -eo pipefail

# use the agenix provided wallet if you have it
if [[ -n "${devnet__keypair__wallet_PATH}" && -f "${devnet__keypair__wallet_PATH}" ]]; then
    WALLET_FILE="${devnet__keypair__wallet_PATH}"
elif [[ -z "${WALLET_FILE:-}" ]]; then
    echo "No wallet file specified, generating ephemeral keypair..."
    # Create a named pipe for the keypair data
    mkdir -p ~/.config/solana/solana-keys
    WALLET_FILE=$(mktemp ~/.config/solana/solana-keys/solana-wallet-XXXXXXXXX)

    # Generate keypair and write to the generated file
    solana-keygen new --no-bip39-passphrase --force --outfile "${WALLET_FILE}"

    echo "Using ephemeral keypair (will not persist after script exits)"
    # Set up cleanup trap to remove the wallet file when script exits
    # This will run on normal exit, SIGINT (Ctrl+C), SIGTERM, or ERR
    trap "echo 'Cleaning up ephemeral wallet file...'; rm -f '${WALLET_FILE}'" EXIT
fi

RPC=${RPC:-"http://127.0.0.1:8899"}
WS_RPC=${WS_RPC:-"ws://127.0.0.1:8900"}
RUN_ID=${RUN_ID:-"test"}
AUTHORIZER=${AUTHORIZER:-"11111111111111111111111111111111"}

# presets for a DGX or an HGX
DP=${DP:-"8"}
TP=${TP:-"1"}
BATCH_SIZE=${BATCH_SIZE:-"1"}

# fine if this fails
solana airdrop 10 "$(solana-keygen pubkey ${WALLET_FILE})" --url "${RPC}" || true

export RUST_LOG="info,psyche=debug"

if [[ "$OTLP_METRICS_URL" == "" ]]; then
    cargo run --release --bin psyche-solana-client -- \
        train \
        --wallet-private-key-path ${WALLET_FILE} \
        --rpc ${RPC} \
        --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} \
        --data-parallelism ${DP} \
        --tensor-parallelism ${TP} \
        --micro-batch-size ${BATCH_SIZE} \
        --authorizer ${AUTHORIZER} \
        --logs "console" \
        "$@"
else
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
        --authorizer ${AUTHORIZER} \
        --otlp-metrics-url "http://localhost:4318/v1/metrics" \
        --otlp-logs-url "http://localhost:4318/v1/logs" \
        "$@"
fi
