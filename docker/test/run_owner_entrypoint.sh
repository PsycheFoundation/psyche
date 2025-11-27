#! /bin/bash

set -o errexit

solana config set --url "${RPC}"
solana-keygen new --no-bip39-passphrase --force
WALLET_FILE="/root/.config/solana/id.json"

solana airdrop 10 "$(solana-keygen pubkey)"

bash /bin/join-authorization-create.sh ${RPC} ${WALLET_FILE} 11111111111111111111111111111111
psyche-solana-client create-run \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}"

psyche-solana-client update-config \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --config-path "/usr/local/config.toml"

psyche-solana-client set-paused \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --resume

echo "all done"
