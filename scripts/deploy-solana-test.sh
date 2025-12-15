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

# Detect if we're deploying to devnet
IS_DEVNET=false
if [[ "$RPC" == *"devnet.solana.com"* ]]; then
    IS_DEVNET=true
fi

echo -e "\n[+] deploy info:"
echo -e "[+] WALLET_FILE = $WALLET_FILE"
echo -e "[+] RPC = $RPC"
echo -e "[+] WS_RPC = $WS_RPC"
echo -e "[+] RUN_ID = $RUN_ID"
echo -e "[+] CONFIG_FILE = $CONFIG_FILE"
echo -e "[+] IS_DEVNET = $IS_DEVNET"
echo -e "[+] DEPLOY_TREASURER = $DEPLOY_TREASURER"
echo -e "[+] -----------------------------------------------------------"

# Deploy Coordinator
echo -e "\n[+] Starting coordinator deploy"
pushd $(pwd)/architectures/decentralized/solana-coordinator

if [[ "$IS_DEVNET" == "true" ]]; then
    echo -e "\n[+] - generating new keypair for devnet..."
    solana-keygen new -o ./target/deploy/psyche_solana_coordinator-keypair.json -f --no-bip39-passphrase
    anchor keys sync
fi

echo -e "\n[+] - building..."
anchor build --no-idl

echo -e "\n[+] - deploying..."
anchor deploy --provider.cluster devnet --provider.wallet ${WALLET_FILE} -- --max-len 500000
sleep 1

echo -e "\n[+] Coordinator program deployed successfully!"
popd

# Deploy Authorizer
echo -e "\n[+] Starting authorizer deploy"
pushd architectures/decentralized/solana-authorizer

if [[ "$IS_DEVNET" == "true" ]]; then
    echo -e "\n[+] - generating new keypair for devnet..."
    solana-keygen new -o ./target/deploy/psyche_solana_authorizer-keypair.json -f --no-bip39-passphrase
    anchor keys sync
fi

echo -e "\n[+] - building..."
anchor build

echo -e "\n[+] - deploying..."
anchor deploy --provider.cluster ${RPC} --provider.wallet ${WALLET_FILE}
sleep 1

echo -e "\n[+] - init-idl..."
AUTHORIZER_PUBKEY=$(solana-keygen pubkey ./target/deploy/psyche_solana_authorizer-keypair.json)
anchor idl init \
    --provider.cluster ${RPC} \
    --provider.wallet ${WALLET_FILE} \
    --filepath target/idl/psyche_solana_authorizer.json \
    ${AUTHORIZER_PUBKEY}

echo -e "\n[+] Authorizer program deployed successfully!"
popd

# Deploy Treasurer (if flag is set)
TREASURER_ARGS=""
if [[ "$DEPLOY_TREASURER" == "true" ]]; then
    echo -e "\n[+] Starting treasurer deploy"
    pushd architectures/decentralized/solana-treasurer

    if [[ "$IS_DEVNET" == "true" ]]; then
        echo -e "\n[+] - generating new keypair for devnet..."
        solana-keygen new -o ./target/deploy/psyche_solana_treasurer-keypair.json -f --no-bip39-passphrase
        anchor keys sync
    fi

    echo -e "\n[+] - building..."
    anchor build

    echo -e "\n[+] - deploying..."
    anchor deploy --provider.cluster ${RPC} --provider.wallet ${WALLET_FILE}
    sleep 1

    echo -e "\n[+] Treasurer program deployed successfully!"
    popd

    # Create token
    echo -e "\n[+] Creating token"
    TOKEN_ADDRESS=$(spl-token create-token --decimals 0 --url ${RPC} | grep "Address:" | awk '{print $2}')
    spl-token create-account ${TOKEN_ADDRESS} --url ${RPC}
    spl-token mint ${TOKEN_ADDRESS} 1000000 --url ${RPC}

    TREASURER_ARGS="--treasurer-collateral-mint ${TOKEN_ADDRESS}"
fi

# Create permisionless authorization
echo -e "\n[+] Creating authorization for everyone to join the run"
bash ./scripts/join-authorization-create.sh ${RPC} ${WALLET_FILE} 11111111111111111111111111111111

echo -e "\n[+] Creating training run..."
cargo run --release --bin psyche-solana-client -- \
    create-run \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --client-version test \
    ${TREASURER_ARGS} \
    "${EXTRA_ARGS[@]}"

echo -e "\n[+] Setting training run earning rate..."
cargo run --release --bin psyche-solana-client -- \
    set-future-epoch-rates \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --earning-rate-total-shared 100.0

echo -e "\n[+] Update training run config..."
cargo run --release --bin psyche-solana-client -- \
    update-config \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --config-path ${CONFIG_FILE} \
    --num-parameters 1100000000 \
    --vocab-size 32768

echo -e "\n[+] Unpause the training run..."
cargo run --release --bin psyche-solana-client -- \
    set-paused \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --resume
