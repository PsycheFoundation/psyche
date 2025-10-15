#! /bin/bash
set -o errexit
solana config set --url "${RPC}"
solana-keygen new --no-bip39-passphrase --force
solana airdrop 10 "$(solana-keygen pubkey)"
echo "Test ${TEST}"
echo "Python enabled ${PYTHON_ENABLED}"


# Build the command based on environment variable
if [ "${PYTHON_ENABLED}" = "true" ]; then
    echo "Starting client with Python features enabled"
    psyche-solana-client train \
        --wallet-private-key-path "/root/.config/solana/id.json" \
        --rpc "${RPC}" \
        --ws-rpc "${WS_RPC}" \
        --run-id "${RUN_ID}" \
        --data-parallelism "8" \
        --logs "json"
else
    echo "Starting client without Python features"
    psyche-solana-client train \
        --wallet-private-key-path "/root/.config/solana/id.json" \
        --rpc "${RPC}" \
        --ws-rpc "${WS_RPC}" \
        --run-id "${RUN_ID}" \
        --logs "json"
fi
