#!/usr/bin/env bash

set -o errexit
set -e
set -m

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
echo -e "[+] -----------------------------------------------------------"

echo -e "\n[+] Starting authorizor deploy"
pushd architectures/decentralized/solana-authorizer

echo -e "\n[+] - building..."
anchor build

echo -e "\n[+] - deploying..."
anchor deploy --provider.cluster ${RPC} --provider.wallet ${WALLET_FILE}
sleep 1 # wait for the program to be deployed and ready in the validator

echo -e "\n[+] - init-idl..."
anchor idl init \
    --provider.cluster ${RPC} \
    --provider.wallet ${WALLET_FILE} \
    --filepath target/idl/psyche_solana_authorizer.json \
    PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw

echo -e "\n[+] Authorizer program deployed successfully!"
popd

echo -e "\n[+] Starting coordinator deploy"
pushd architectures/decentralized/solana-coordinator

echo -e "\n[+] - building..."
anchor build --no-idl

echo -e "\n[+] - deploying..."
anchor deploy --provider.cluster ${RPC} --provider.wallet ${WALLET_FILE} -- --max-len 500000
sleep 1 # wait for the program to be deployed and ready in the validator

echo -e "\n[+] Coordinator program deployed successfully!"
popd

echo -e "\n[+] Starting treasurer deploy"
pushd architectures/decentralized/solana-treasurer

echo -e "\n[+] - building..."
anchor build --no-idl

echo -e "\n[+] - deploying..."
anchor deploy --provider.cluster ${RPC} --provider.wallet ${WALLET_FILE} -- --max-len 500000
sleep 1 # wait for the program to be deployed and ready in the validator

echo -e "\n[+] Treasurer program deployed successfully!"
popd

echo -e "\n[+] Creating authorization for everyone to join the run"
cargo run --release --bin psyche-solana-client -- \
    join-authorization-create \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --authorizer 11111111111111111111111111111111

echo -e "\n[+] Creating training run..."
cargo run --release --bin psyche-solana-client -- \
    create-run \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --treasurer-collateral-mint So11111111111111111111111111111111111111112 \
    --client-version "test" \
    --run-id ${RUN_ID} "$@"

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
