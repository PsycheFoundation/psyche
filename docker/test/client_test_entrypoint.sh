#! /bin/bash
set -o errexit
solana config set --url "${RPC}"
solana-keygen new --no-bip39-passphrase --force
solana airdrop 10 "$(solana-keygen pubkey)"
echo "Python enabled ${PYTHON_ENABLED}"

SIDECAR_PORT=$(shuf -i 9000-9100 -n 1)
echo "USING SIDECAR PORT: ${SIDECAR_PORT}"

# Build the command based on environment variable
if [ "${PYTHON_ENABLED}" = "true" ]; then
    echo "Starting client with Python features enabled"
    psyche-solana-client train \
        --wallet-private-key-path "/root/.config/solana/id.json" \
        --rpc "${RPC}" \
        --ws-rpc "${WS_RPC}" \
        --run-id "${RUN_ID}" \
        --data-parallelism 8 \
        --sidecar-port "${SIDECAR_PORT}" \
        --model-extra-data-toml "/usr/local/config.toml" \
        --logs "json"
else
    echo "Starting client without Python features"
    psyche-solana-client train \
        --wallet-private-key-path "/root/.config/solana/id.json" \
        --rpc "${RPC}" \
        --ws-rpc "${WS_RPC}" \
        --run-id "${RUN_ID}" \
        --model-extra-data-toml "/usr/local/config.toml" \
        --logs "json"
fi
