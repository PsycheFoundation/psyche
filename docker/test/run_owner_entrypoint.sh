#! /bin/bash

set -o errexit

solana config set --url "${RPC}"
solana-keygen new --no-bip39-passphrase --force
WALLET_FILE="/root/.config/solana/id.json"

solana airdrop 10 "$(solana-keygen pubkey)"

echo -e "\n[+] - init-idl..."
pushd /local/solana-authorizer
anchor idl init \
    --provider.cluster ${RPC} \
    --provider.wallet ${WALLET_FILE} \
    --filepath /local/solana-authorizer/target/idl/psyche_solana_authorizer.json \
    PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw

echo -e "\n[+] Creating authorization for everyone to join the run"
popd

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
