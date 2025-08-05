#!/bin/bash

set -o errexit
set -e
set -m

_usage() {
    echo "Usage: $0 <SOLANA_RPC> <SENDER_KEYPAIR_FILE> <RUN_ID> <COLLATERAL_AMOUNT>"
    echo "  SOLANA_RPC: The solana RPC url or moniker to use"
    echo "  SENDER_KEYPAIR_FILE: The keypair file of the payer"
    echo "  RUN_ID: The run ID"
    echo "  COLLATERAL_AMOUNT: The amount of collateral token to deposit"
    exit 1
}

if [[ "$#" -lt 4 ]]; then
    _usage
fi

SOLANA_RPC="$1"
shift

SENDER_KEYPAIR_FILE="$1"
shift

if [[ ! -f "$SENDER_KEYPAIR_FILE" ]]; then
    echo "Error: Payer keypair file '$SENDER_KEYPAIR_FILE' not found."
    _usage
fi

RUN_ID="$1"
shift

COLLATERAL_AMOUNT="$1"
shift

# Make sure all is good to go
echo "SOLANA_RPC: $SOLANA_RPC"
echo "SENDER_KEYPAIR_FILE: $SENDER_KEYPAIR_FILE"
echo "RUN_ID: $RUN_ID"
echo "COLLATERAL_AMOUNT: $COLLATERAL_AMOUNT"

echo "----"
echo "Fetch run info..."
RUN_INFO=$( \
        cargo run --release --bin psyche-solana-client -- \
        info \
        --rpc $SOLANA_RPC \
        --run-id "$RUN_ID"
)

echo "----"
echo "Extract treasurer fields..."
TREASURER_RUN_ADDRESS=$(echo $RUN_INFO | jq -r '.treasurer_run.address')
echo "TREASURER_RUN_ADDRESS: $TREASURER_RUN_ADDRESS"
TREASURER_RUN_COLLATERAL_MINT=$(echo $RUN_INFO | jq -r '.treasurer_run.collateral_mint')
echo "TREASURER_RUN_COLLATERAL_MINT: $TREASURER_RUN_COLLATERAL_MINT"

echo "----"
echo "Deposit collateral..."
spl-token transfer \
    --owner $SENDER_KEYPAIR_FILE \
    $TREASURER_RUN_COLLATERAL_MINT \
    $COLLATERAL_AMOUNT \
    $TREASURER_RUN_ADDRESS \
    --allow-non-system-account-recipient
