#! /bin/bash

set -o errexit

solana config set --url "${RPC}"
solana-keygen new --no-bip39-passphrase --force
WALLET_FILE="/root/.config/solana/id.json"

solana airdrop 10 "$(solana-keygen pubkey)"

run-manager join-authorization-create \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --authorizer 11111111111111111111111111111111

run-manager create-run \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --client-version "latest"

run-manager update-config \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --config-path "/usr/local/config.toml" \
    --skip-upload-model-extra-data

run-manager set-paused \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --resume
