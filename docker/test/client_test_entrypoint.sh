#! /bin/bash
set -o errexit
solana config set --url "${RPC}"

WALLET_FILE="/root/.config/solana/id.json"

# Generate keypair only if it doesn't exist (allows mounting via volume bind)
if [ ! -f "${WALLET_FILE}" ]; then
    echo "Generating new client keypair"
    solana-keygen new --no-bip39-passphrase --force
else
    echo "Using existing client keypair"
fi

solana airdrop 10 "$(solana-keygen pubkey)"
echo "Python enabled ${PYTHON_ENABLED}"

SIDECAR_PORT=$(shuf -i 9000-9100 -n 1)
echo "USING SIDECAR PORT: ${SIDECAR_PORT}"

# Build the command based on environment variable
if [ "${PYTHON_ENABLED}" = "true" ]; then
    echo "Starting client with Python features enabled"
    psyche-solana-client train \
        --wallet-private-key-path "${WALLET_FILE}" \
        --rpc "${RPC}" \
        --ws-rpc "${WS_RPC}" \
        --run-id "${RUN_ID}" \
        --data-parallelism 8 \
        --sidecar-port "${SIDECAR_PORT}" \
        --logs "json"
else
    echo "Starting client without Python features"
    psyche-solana-client train \
        --wallet-private-key-path "${WALLET_FILE}" \
        --rpc "${RPC}" \
        --ws-rpc "${WS_RPC}" \
        --run-id "${RUN_ID}" \
        --logs "json"
fi

