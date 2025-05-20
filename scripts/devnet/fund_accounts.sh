#!/bin/bash

# This script creates some test accounts in Solana Devnet and, given an account with balance,
# distributes SOL between each account. One can also provide a list of recipient accounts
# instead of creating them.
# Usage:
# fund_accounts.sh <SENDER_KEYPAIR> <NUM_ACCOUNTS> (this will create the recipient accounts automatically)
# fund_accounts.sh <SENDER_KEYPAIR> <NUM_ACCOUNTS> <RECIPIENTS_FILE> (this will use the pubkeys specified in the text file)

# Remember to set Solana CLI to Devnet
# solana config set --url https://api.devnet.solana.com

set -o errexit

AMOUNT="0.1" # Amount of SOL to send to each recipient after airdrop

_usage() {
    echo "Usage: $0 <SENDER_KEYPAIR> <NUM_ACCOUNTS> optional:[RECIPIENTS_FILE]"
    echo "  SENDER_KEYPAIR: Path to the sender keypair file (e.g., devnet-wallet.json)"
    echo "  NUM_ACCOUNTS: Number of accounts to create (if RECIPIENTS_FILE is present this will be ignored)"
    echo "  RECIPIENTS_FILE: Optional. Path to the file containing existing pubkeys"
    exit 1
}

_continue?() {
    read -p "Continue? [y/N] " answer
    if [[ "$answer" =~ ^[Yy]$ || -z "$answer" ]]; then
        echo "Continuing..."
    else
        echo "Exiting."
        exit 1
    fi
}

if [[ "$#" -lt 2 || "$#" -gt 3 ]]; then
    _usage
fi

echo -e "\nThis is the current Solana configuration. Ensure that everything is OK:\n"
solana config get
_continue?

SENDER_KEYPAIR="$1"
NUM_ACCOUNTS="$2"
RECIPIENTS_FILE="$3"

# Ensure the sender keypair file exists
if [[ ! -f "$SENDER_KEYPAIR" ]]; then
    echo "Error: Sender keypair file '$SENDER_KEYPAIR' not found."
    _usage
fi

# Ensure NUM_ACCOUNTS is a positive integer
if ! [[ "$NUM_ACCOUNTS" =~ ^[1-9][0-9]*$ ]]; then
    echo "Error: NUM_ACCOUNTS must be a positive integer."
    _usage
fi

# If recipients file is not provided, generate new keypairs
if [[ -z "$RECIPIENTS_FILE" ]]; then
    echo "Recipients file not provided. The script will generate $NUM_ACCOUNTS new keypairs"
    _continue?

    KEY_DIR="devnet_funded_accounts"
    mkdir -p "$KEY_DIR"
    RECIPIENTS_FILE="$KEY_DIR/pubkeys.txt"
    > "$RECIPIENTS_FILE" # Clear the file if it exists

    for ((i=1; i<=NUM_ACCOUNTS; i++)); do
        KEYPAIR_FILE="$KEY_DIR/keypair_$i.json"
        solana-keygen new --no-passphrase --outfile "$KEYPAIR_FILE" --silent
        PUBKEY=$(solana-keygen pubkey "$KEYPAIR_FILE")
        echo "$PUBKEY" >> "$RECIPIENTS_FILE"
        echo "Generated keypair $i: $PUBKEY"
    done

    echo "Generated keypairs saved to $KEY_DIR/ and pubkeys listed in $RECIPIENTS_FILE."
fi

# Ensure recipients file exists
if [[ ! -f "$RECIPIENTS_FILE" ]]; then
    echo "Error: Recipients file '$RECIPIENTS_FILE' not found."
    exit 1
fi

accounts_in_file=$(grep -cve '^$' "$RECIPIENTS_FILE")

# Ensure the sender keypair file exists
if [[ "$accounts_in_file" -lt "$NUM_ACCOUNTS" ]]; then
    echo "Error: Not enough accounts in accounts file"
    exit 1
fi

echo -e "\nWe will be using a total of (${NUM_ACCOUNTS} * ${AMOUNT}) SOL for funding accounts"
_continue?

# Fund each recipient with the specified amount of SOL
echo "Funding recipients..."
while IFS= read -r RECIPIENT; do
    if [[ -z "$RECIPIENT" ]]; then # Skip empty lines
        continue
    fi

    echo "Sending $AMOUNT SOL to $RECIPIENT..."

    solana transfer "$RECIPIENT" "$AMOUNT" --from "$SENDER_KEYPAIR" \
        --fee-payer "$SENDER_KEYPAIR" --allow-unfunded-recipient

    # Check if the transfer was successful
    if [[ $? -eq 0 ]]; then
        echo "Success: Sent $AMOUNT SOL to $RECIPIENT"
    else
        echo "Error: Failed to send $AMOUNT SOL to $RECIPIENT"
    fi

    echo "----------------------------------------"
done < "$RECIPIENTS_FILE"

echo "Funding complete."
