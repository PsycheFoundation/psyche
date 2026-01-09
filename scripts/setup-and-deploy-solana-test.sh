#! /usr/bin/env bash

set -o errexit
set -e
set -m

# Parse command line arguments
DEPLOY_TREASURER=false
EXTRA_ARGS=()

for arg in "$@"; do
    if [[ "$arg" == "--treasurer" ]]; then
        DEPLOY_TREASURER=true
    else
        EXTRA_ARGS+=("$arg")
    fi
done

RPC=${RPC:-"http://127.0.0.1:8899"}
CONFIG_FILE=${CONFIG_FILE:-"./config/solana-test/config.toml"}
# use the agenix provided wallet if you have it
if [[ -n "${devnet__keypair__wallet_PATH}" && -f "${devnet__keypair__wallet_PATH}" ]]; then
    DEFAULT_WALLET="${devnet__keypair__wallet_PATH}"
else
    DEFAULT_WALLET="$HOME/.config/solana/id.json"
fi
WALLET_FILE=${KEY_FILE:-"$DEFAULT_WALLET"}

cleanup() {
    echo -e "\nCleaning up background processes...\n"
    kill $(jobs -p) 2>/dev/null
    wait
}

trap cleanup INT EXIT
solana-test-validator --limit-ledger-size 10000000 -r 1>/dev/null &
echo -e "\n[+] Started test validator!"

sleep 3

solana airdrop 10 --url ${RPC} --keypair ${WALLET_FILE}

# Pass treasurer flag to deploy script if set
if [[ "$DEPLOY_TREASURER" == "true"  && "$PERMISSIONLESS" == "true" ]]; then
    WALLET_FILE=${WALLET_FILE} ./scripts/deploy-solana-test.sh --treasurer "${EXTRA_ARGS[@]}"
    CONFIG_FILE=${CONFIG_FILE} WALLET_FILE=${WALLET_FILE} ./scripts/create-permissionless-run.sh --treasurer "${EXTRA_ARGS[@]}"
elif [[ "$PERMISSIONLESS" == "true"  ]]; then
    WALLET_FILE=${WALLET_FILE} ./scripts/deploy-solana-test.sh "${EXTRA_ARGS[@]}"
    CONFIG_FILE=${CONFIG_FILE} WALLET_FILE=${WALLET_FILE} ./scripts/create-permissionless-run.sh "${EXTRA_ARGS[@]}"
fi
echo -e "\n[+] Testing Solana setup ready, starting Solana logs...\n"

solana logs --url ${RPC}
