#! /bin/bash

set -o errexit
set -euo pipefail

# Read from environment variables with defaults
RUN_ID=${RUN_ID:-"test"}
WALLET_FILE=${WALLET_FILE:-"/tmp/run-owner-keypair.json"}
RPC=${RPC:-"http://psyche-solana-test-validator:8899"}
WS_RPC=${WS_RPC:-"ws://psyche-solana-test-validator:8900"}

echo "[+] Pause entrypoint script"
echo "[+] RUN_ID = $RUN_ID"
echo "[+] WALLET_FILE = $WALLET_FILE"
echo "[+] RPC = $RPC"
echo "[+] WS_RPC = $WS_RPC"

# Airdrop to the wallet
echo "[+] Airdropping SOL to wallet..."
solana airdrop 10 --url ${RPC} --keypair ${WALLET_FILE}

# Run the pause command (without --resume flag)
CMD="run-manager set-paused --wallet-private-key-path ${WALLET_FILE} --rpc ${RPC} --ws-rpc ${WS_RPC} --run-id ${RUN_ID}"
echo "[+] Running: $CMD"
$CMD

echo "[+] Pause complete!"
