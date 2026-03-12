#!/usr/bin/env bash

set -o errexit

# This script sets up a test run by creating the run and configuring it
# It runs from the host (not inside docker) and connects to the validator
# running in docker-compose

# Load env vars from the test config
if [ -f "config/client/.env.local" ]; then
    source config/client/.env.local
else
    echo "[!] config/client/.env.local not found"
    exit 1
fi

# Use RPC and WS_RPC from env file (should point to localhost:8899/8900)
RPC="http://127.0.0.1:8899"
WS_RPC="ws://127.0.0.1:8900"
RUN_ID=${RUN_ID:-"test"}

# Check if an owner keypair path was provided, otherwise create a temporary one
if [ -n "${OWNER_KEYPAIR_PATH}" ] && [ -f "${OWNER_KEYPAIR_PATH}" ]; then
    echo "[+] Using provided owner keypair: ${OWNER_KEYPAIR_PATH}"
    WALLET_FILE="${OWNER_KEYPAIR_PATH}"
    CLEANUP_WALLET=false
else
    # Create a temporary wallet for the run owner
    TEMP_DIR=$(mktemp -d)
    WALLET_FILE="${TEMP_DIR}/id.json"
    CLEANUP_WALLET=true

    echo "[+] Generating temporary wallet..."
    solana-keygen new --no-bip39-passphrase --force --outfile "${WALLET_FILE}"
fi

echo "[+] Configuring solana CLI..."
solana config set --url "${RPC}"

echo "[+] Airdropping SOL to wallet..."
solana airdrop 10 "$(solana-keygen pubkey ${WALLET_FILE})" --url "${RPC}"

echo "[+] Creating join authorization..."
nix run .#run-manager -- \
    join-authorization-create \
    --wallet-private-key-path "${WALLET_FILE}" \
    --rpc "${RPC}" \
    --authorizer 11111111111111111111111111111111

echo "[+] Creating run..."
nix run .#run-manager -- \
    create-run \
    --wallet-private-key-path "${WALLET_FILE}" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --client-version "latest"

echo "[+] Updating config..."
nix run .#run-manager -- \
    update-config \
    --wallet-private-key-path "${WALLET_FILE}" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --config-path "config/solana-test/test-config.toml" \
    --skip-upload-model-extra-data

echo "[+] Unpausing run..."
nix run .#run-manager -- \
    set-paused \
    --wallet-private-key-path "${WALLET_FILE}" \
    --rpc "${RPC}" \
    --ws-rpc "${WS_RPC}" \
    --run-id "${RUN_ID}" \
    --resume

echo "[+] Test run setup complete!"

# Clean up temporary wallet if we created one
if [ "${CLEANUP_WALLET}" = true ]; then
    rm -rf "${TEMP_DIR}"
fi
