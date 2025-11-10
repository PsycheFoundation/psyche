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

psyche-solana-client train \
    --wallet-private-key-path "${WALLET_FILE}" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --logs "json"
