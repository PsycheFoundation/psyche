#!/bin/bash

set -o errexit

solana config set --url "${RPC}"
solana-keygen new --no-bip39-passphrase --force

solana airdrop 10 "$(solana-keygen pubkey)"

psyche-solana-client train \
    --wallet-private-key-path "/root/.config/solana/id.json" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --ticker \
    --logs "json"
