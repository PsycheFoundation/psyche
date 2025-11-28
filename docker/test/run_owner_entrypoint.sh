#! /bin/bash

set -o errexit

solana config set --url "${RPC}"

# Use pre-mounted keypair if available, otherwise generate a new one
if [ -f "/usr/local/keypairs/run_owner.json" ]; then
    # Copy pre-mounted keypair to default location for solana-toolbox compatibility
    mkdir -p /root/.config/solana
    cp /usr/local/keypairs/run_owner.json /root/.config/solana/id.json
    WALLET_FILE="/root/.config/solana/id.json"
    echo "Using pre-mounted run owner keypair"
    solana airdrop 10 "$(solana-keygen pubkey)"
else
    solana-keygen new --no-bip39-passphrase --force
    WALLET_FILE="/root/.config/solana/id.json"
    echo "Generated new run owner keypair"
    solana airdrop 10 "$(solana-keygen pubkey)"
fi

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
