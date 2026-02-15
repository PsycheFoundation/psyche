#!/usr/bin/env bash

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

# use the agenix provided wallet if you have it
if [[ -n "${devnet__keypair__wallet_PATH}" && -f "${devnet__keypair__wallet_PATH}" ]]; then
    DEFAULT_WALLET="${devnet__keypair__wallet_PATH}"
else
    DEFAULT_WALLET="$HOME/.config/solana/id.json"
fi
WALLET_FILE=${KEY_FILE:-"$DEFAULT_WALLET"}
RPC=${RPC:-"http://127.0.0.1:8899"}
WS_RPC=${WS_RPC:-"ws://127.0.0.1:8900"}
RUN_ID=${RUN_ID:-"test"}
CONFIG_FILE=${CONFIG_FILE:-"./config/solana-test/config.toml"}

echo -e "\n[+] deploy info:"
echo -e "[+] WALLET_FILE = $WALLET_FILE"
echo -e "[+] RPC = $RPC"
echo -e "[+] WS_RPC = $WS_RPC"
echo -e "[+] RUN_ID = $RUN_ID"
echo -e "[+] CONFIG_FILE = $CONFIG_FILE"
echo -e "[+] DEPLOY_TREASURER = $DEPLOY_TREASURER"
echo -e "[+] -----------------------------------------------------------"

# Create permisionless authorization
echo -e "\n[+] Creating authorization for everyone to join the run"
cargo run --release --bin run-manager -- \
    join-authorization-create \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "${RPC}" \
    --authorization permissionless

echo -e "\n[+] Creating training run..."
cargo run --release --bin run-manager -- \
    create-run \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --client-version test \
    ${TREASURER_ARGS} \
    "${EXTRA_ARGS[@]}"

if [[ "$DEPLOY_TREASURER" == "true" ]]; then
    echo -e "\n[+] Setting treasurer collateral requirements..."
    cargo run --release --bin run-manager treasurer-top-up-rewards \
        --run-id ${RUN_ID} \
        --collateral-amount 10 \
        --wallet-private-key-path ${WALLET_FILE} \
        --rpc ${RPC}

    cargo run --release --bin run-manager -- set-future-epoch-rates \
        --rpc ${RPC} \
        --run-id ${RUN_ID} \
        --wallet-private-key-path ${WALLET_FILE} \
        --earning-rate-total-shared 10 \
        --slashing-rate-per-client 10
fi

echo -e "\n[+] Update training run config..."
cargo run --release --bin run-manager -- \
    update-config \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --config-path ${CONFIG_FILE} \
    --num-parameters 1100000000 \
    --vocab-size 32768

echo -e "\n[+] Unpause the training run..."
cargo run --release --bin run-manager -- \
    set-paused \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --resume
