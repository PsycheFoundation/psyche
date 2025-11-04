#!/bin/bash

set -o errexit

solana config set --url "${RPC}"
# NOTE: solana-keygen new is SKIPPED because keypair is mounted via volume bind

solana airdrop 10 "$(solana-keygen pubkey)"

psyche-solana-client train \
    --wallet-private-key-path "/root/.config/solana/id.json" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --logs "json"
