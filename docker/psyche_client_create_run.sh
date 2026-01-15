#! /bin/bash

set -o errexit

env_path="./config/client/.env"

if [[ ! -f "$env_path" ]]; then
    echo -e "\nEnvironment file does not exist. You must provide one."
    exit 1
fi

source "$env_path"

if [[ "$WALLET_FILE" == "" ]]; then
    echo -e "\n[!] The WALLET_FILE env variable was not set."
    exit 1
fi

if [[ ! -f "$WALLET_FILE" ]]; then
    echo -e "\n[!] The file that was set in the WALLET_FILE env variable does not exist."
    exit 1
fi

if [[ "$RPC" == "" ]]; then
    echo -e "\n[!] The RPC env variable was not set."
    exit 1
fi

if [[ "$WS_RPC" == "" ]]; then
    echo -e "\n[!] The WS_RPC env variable was not set."
    exit 1
fi

if [[ "$RUN_ID" == "" ]]; then
    echo -e "\n[!] The RUN_ID env variable was not set."
    exit 1
fi

if [[ "$CONFIG_PATH" == "" ]]; then
    echo -e "\n[!] The CONFIG_PATH env variable was not set."
    exit 1
fi

if [[ ! -f "$CONFIG_PATH" ]]; then
    echo -e "\n[!] The file that was set in the CONFIG_PATH env variable does not exist."
    echo -e "File does not exist: ${CONFIG_PATH}"
    exit 1
fi

echo -e "\n[+] Creating training run with run ID '${RUN_ID}'"
cargo run --release --bin run-manager -- \
    create-run \
    --wallet-private-key-path "${WALLET_FILE}" \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --client-version "latest"

echo -e "\n[+] Training run created successfully!"
echo -e "\n[+] Uploading model config..."

cargo run --release --bin run-manager -- \
    update-config \
    --wallet-private-key-path "${WALLET_FILE}" \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --config-path "${CONFIG_PATH}"

echo -e "\n[+] Model config uploaded successfully"

cargo run --release --bin run-manager -- \
    set-paused \
    --wallet-private-key-path "${WALLET_FILE}" \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --resume

echo -e "\n[+] Training run with run ID '${RUN_ID}' was set up successfully!"
