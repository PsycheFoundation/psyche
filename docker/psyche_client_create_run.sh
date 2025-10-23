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
docker run --rm -v "$WALLET_FILE":/keys/id.json \
    --add-host=host.docker.internal:host-gateway \
    --entrypoint /usr/local/bin/psyche-solana-client \
    psyche-client \
    create-run \
    --wallet-private-key-path "/keys/id.json" \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --client-version "latest"

echo -e "\n[+] Training run created successfully!"
echo -e "\n[+] Uploading model config..."

docker run --rm -v "$WALLET_FILE":/keys/id.json \
    -v "$CONFIG_PATH":/model_config/config.toml \
    --add-host=host.docker.internal:host-gateway \
    --entrypoint /usr/local/bin/psyche-solana-client \
    psyche-client \
    update-config \
    --wallet-private-key-path "/keys/id.json" \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --config-path "/model_config/config.toml"

echo -e "\n[+] Model config uploaded successfully"

docker run --rm -v "$WALLET_FILE":/keys/id.json \
    --add-host=host.docker.internal:host-gateway \
    --entrypoint /usr/local/bin/psyche-solana-client \
    psyche-client \
    set-paused \
    --wallet-private-key-path "/keys/id.json" \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --resume

echo -e "\n[+] Training run with run ID '${RUN_ID}' was set up successfully!"
