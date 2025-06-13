#! /bin/bash

set -o errexit

solana config set --url "${RPC}"
solana-keygen new --no-bip39-passphrase --force

solana airdrop 10 "$(solana-keygen pubkey)"

psyche-solana-client create-run \
    --wallet-private-key-path "/root/.config/solana/id.json" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}"

psyche-solana-client update-config \
    --wallet-private-key-path "/root/.config/solana/id.json" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --config-path "/usr/local/config.toml"

psyche-solana-client set-paused \
    --wallet-private-key-path "/root/.config/solana/id.json" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --resume
