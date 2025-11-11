#!/usr/bin/env bash

set -o errexit
set -e

# Parameters: RUN_ID PAUSED WALLET_FILE RPC WS_RPC
RUN_ID=${1:-"test"}
PAUSED=${2:-"true"}
WALLET_FILE=${3:-"/tmp/run-owner-keypair.json"}
RPC=${4:-"http://psyche-solana-test-validator:8899"}
WS_RPC=${5:-"ws://psyche-solana-test-validator:8900"}

echo "[+] Set-paused script"
echo "[+] RUN_ID = $RUN_ID"
echo "[+] PAUSED = $PAUSED"
echo "[+] WALLET_FILE = $WALLET_FILE"
echo "[+] RPC = $RPC"
echo "[+] WS_RPC = $WS_RPC"

# Airdrop to the wallet
echo "[+] Airdropping SOL to wallet..."
solana airdrop 10 --url ${RPC} --keypair ${WALLET_FILE}

# Run the pause command
CMD="psyche-solana-client set-paused --wallet-private-key-path ${WALLET_FILE} --rpc ${RPC} --ws-rpc ${WS_RPC} --run-id ${RUN_ID}"
if [ "$PAUSED" = "false" ]; then
    CMD="$CMD --resume"
fi
echo "[+] Running: $CMD"
$CMD

echo "[+] Set-paused complete!"
