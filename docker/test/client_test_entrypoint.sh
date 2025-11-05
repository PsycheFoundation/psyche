#! /bin/bash

set -o errexit

solana config set --url "${RPC}"

# Use pre-mounted keypair if available, otherwise generate a new one
if [ -f "/usr/local/keypairs/client.json" ]; then
    # Copy pre-mounted keypair to default location for solana-toolbox compatibility
    mkdir -p /root/.config/solana
    cp /usr/local/keypairs/client.json /root/.config/solana/id.json
    WALLET_FILE="/root/.config/solana/id.json"
    echo "Using pre-mounted client keypair"
    solana airdrop 10 "$(solana-keygen pubkey)"
else
    solana-keygen new --no-bip39-passphrase --force
    WALLET_FILE="/root/.config/solana/id.json"
    echo "Generated new client keypair"
    solana airdrop 10 "$(solana-keygen pubkey)"
fi

psyche-solana-client train \
    --wallet-private-key-path "${WALLET_FILE}" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --logs "json"
