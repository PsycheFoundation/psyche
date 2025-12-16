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

echo -e "\n[+] Starting coordinator deploy"
pushd architectures/decentralized/solana-coordinator
solana-keygen new -o ./target/deploy/psyche_solana_coordinator-keypair.json -f --no-bip39-passphrase
anchor keys sync

echo -e "\n[+] - building..."
anchor build --no-idl

echo -e "\n[+] - deploying..."
anchor deploy --provider.cluster devnet --provider.wallet ${WALLET_FILE} -- --max-len 500000
sleep 1 # wait for the program to be deployed and ready in the validator

echo -e "\n[+] Coordinator program deployed successfully!"
popd

echo -e "\n[+] Starting authorizor deploy"
pushd architectures/decentralized/solana-authorizer

solana-keygen new -o ./target/deploy/psyche_solana_authorizer-keypair.json -f --no-bip39-passphrase
anchor keys sync

echo -e "\n[+] - building..."
anchor build

echo -e "\n[+] - deploying..."
anchor deploy --provider.cluster devnet --provider.wallet ${WALLET_FILE}
sleep 1 # wait for the program to be deployed and ready in the validator

echo -e "\n[+] - init-idl..."
anchor idl init \
    --provider.cluster devnet \
    --provider.wallet ${WALLET_FILE} \
    --filepath target/idl/psyche_solana_authorizer.json \
    $(solana-keygen pubkey ./target/deploy/psyche_solana_authorizer-keypair.json)

echo -e "\n[+] Authorizer program deployed successfully!"
popd

# echo -e "\n[+] Starting treasurer deploy"
# pushd architectures/decentralized/solana-treasurer
# solana-keygen new -o ./target/deploy/psyche_solana_treasurer-keypair.json -f --no-bip39-passphrase
# anchor keys sync

# echo -e "\n[+] - building..."
# anchor build

# echo -e "\n[+] - deploying..."
# anchor deploy --provider.cluster devnet --provider.wallet ${WALLET_FILE}
# sleep 1 # wait for the program to be deployed and ready in the validator
# echo -e "\n[+] Treasurer program deployed successfully!"
# popd

echo -e "\n[+] Creating authorization for everyone to join the run"
bash ./scripts/join-authorization-create.sh "https://api.devnet.solana.com" ${WALLET_FILE} 11111111111111111111111111111111

# echo -e "\n[+] Creating token"
# TOKEN_ADDRESS=$(spl-token create-token --decimals 0 --url "https://api.devnet.solana.com" | grep "Address:" | awk '{print $2}')
# spl-token create-account ${TOKEN_ADDRESS} --url "https://api.devnet.solana.com"
# spl-token mint ${TOKEN_ADDRESS} 1000000 --url "https://api.devnet.solana.com"

echo -e "\n[+] Creating training run..."
cargo run --release --bin psyche-solana-client -- \
    create-run \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "https://api.devnet.solana.com" \
    --ws-rpc "wss://api.devnet.solana.com" \
    --client-version "test" \
    --run-id ${RUN_ID} "$@"

echo -e "\n[+] Update training run config..."
cargo run --release --bin psyche-solana-client -- \
    update-config \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc "https://api.devnet.solana.com" \
    --ws-rpc "wss://api.devnet.solana.com" \
    --run-id ${RUN_ID} \
    --config-path ${CONFIG_FILE}

# echo -e "\n[+] Update training run model..."
# cargo run --release --bin psyche-solana-client -- \
    #     update-model \
    #     --wallet-private-key-path ${WALLET_FILE} \
    #     --rpc "https://api.devnet.solana.com" \
    #     --ws-rpc "wss://api.devnet.solana.com" \
    #     --run-id ${RUN_ID} \
    #     --config-path ${CONFIG_FILE}

# echo -e "\n[+] Unpause the training run..."
# cargo run --release --bin psyche-solana-client -- \
    #     set-paused \
    #     --wallet-private-key-path ${WALLET_FILE} \
    #     --rpc "https://api.devnet.solana.com" \
    #     --ws-rpc "wss://api.devnet.solana.com" \
    #     --run-id ${RUN_ID} \
    #     --resume
