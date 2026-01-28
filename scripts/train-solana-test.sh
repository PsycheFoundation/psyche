#!/usr/bin/env bash
set -eo pipefail

# use the agenix provided wallet if you have it
if [[ -n "${devnet__keypair__wallet_PATH}" && -f "${devnet__keypair__wallet_PATH}" ]]; then
    WALLET_FILE="${devnet__keypair__wallet_PATH}"
elif [[ -z "${WALLET_FILE:-}" ]]; then
    echo "No wallet file specified, generating ephemeral keypair..."
    mkdir -p ~/.config/solana/solana-keys
    WALLET_FILE=$(mktemp ~/.config/solana/solana-keys/solana-wallet-XXXXXXXXX)

    solana-keygen new --no-bip39-passphrase --force --outfile "${WALLET_FILE}"

    echo "Using ephemeral keypair (will not persist after script exits)"
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

# Optional checkpoint args
CHECKPOINT_ARGS=()
if [[ "$CHECKPOINT" == "true" ]]; then
    echo -e "\n[+] Starting Solana training with checkpointing enabled..."
    CHECKPOINT_ARGS+=(--skip-checkpoint-upload)
else
    echo -e "\n[+] Starting Solana training without checkpointing..."
fi

# fine if this fails
solana airdrop 10 "$(solana-keygen pubkey "${WALLET_FILE}")" --url "${RPC}" || true

export RUST_LOG="info,psyche=debug"

COMMON_ARGS=(
    train
    --wallet-private-key-path "${WALLET_FILE}"
    --rpc "${RPC}"
    --ws-rpc "${WS_RPC}"
    --run-id "${RUN_ID}"
    --data-parallelism "${DP}"
    --tensor-parallelism "${TP}"
    --micro-batch-size "${BATCH_SIZE}"
    --authorizer "${AUTHORIZER}"
    --logs console
    "${CHECKPOINT_ARGS[@]}"
    "$@"
)

if [[ -z "${OTLP_METRICS_URL:-}" ]]; then
    HF_TOKEN=${HF_TOKEN} \
        GOOGLE_APPLICATION_CREDENTIALS=${GOOGLE_APPLICATION_CREDENTIALS} \
        cargo run --release --bin psyche-solana-client -- \
        "${COMMON_ARGS[@]}"
else
    HF_TOKEN=${HF_TOKEN} \
        GOOGLE_APPLICATION_CREDENTIALS=${GOOGLE_APPLICATION_CREDENTIALS} \
        cargo run --release --bin psyche-solana-client -- \
        "${COMMON_ARGS[@]}" \
        --oltp-metrics-url "http://localhost:4318/v1/metrics" \
        --oltp-logs-url "http://localhost:4318/v1/logs"
fi
