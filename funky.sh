#!/usr/bin/env bash

set -o errexit
set -e
set -m

# use the agenix provided wallet if you have it
DEFAULT_WALLET=${devnet__master__keypair__wallet_PATH:-"$HOME/.config/solana/id.json"}
#DEFAULT_WALLET=${devnet__keypair__wallet_PATH:-"$HOME/plaintext-2/devnet_funded_accounts/keypair_3.json"}
WALLET_FILE=${KEY_FILE:-"$DEFAULT_WALLET"}
RPC=${RPC:-"https://devnet.helius-rpc.com/?api-key=73970171-7c76-4e93-85f9-7042d1ab6722"}
WS_RPC=${WS_RPC:-"wss://devnet.helius-rpc.com/?api-key=73970171-7c76-4e93-85f9-7042d1ab6722"}
#RPC=${RPC:-"https://nameless-morning-firefly.solana-devnet.quiknode.pro/f3d4f9fee75006a3e5dead4f570b26788e458ac3"}
#WS_RPC=${WS_RPC:-"wss://nameless-morning-firefly.solana-devnet.quiknode.pro/f3d4f9fee75006a3e5dead4f570b26788e458ac3"}
RUN_ID=${RUN_ID:-"hermes-4-8b"}
CONFIG_FILE=${CONFIG_FILE:-"./data/hermes-4-8b.toml"}

echo -e "\n[+] deploy info:"
echo -e "[+] WALLET_FILE = $WALLET_FILE"
echo -e "[+] RPC = $RPC"
echo -e "[+] WS_RPC = $WS_RPC"
echo -e "[+] RUN_ID = $RUN_ID"
echo -e "[+] CONFIG_FILE = $CONFIG_FILE"
echo -e "[+] -----------------------------------------------------------"

# echo -e "\n[+] starting authorizor deploy"
# pushd architectures/decentralized/solana-authorizer
# echo -e "\n[+] syncing keys..."
# anchor keys sync --provider.cluster ${RPC} --provider.wallet $WALLET_FILE

# echo -e "\n[+] building..."
# anchor build --no-idl

# echo -e "\n[+] deploying..."
# anchor deploy --provider.cluster ${RPC} --provider.wallet $WALLET_FILE -- --max-len 500000
# popd
# echo -e "\n[+] Authorizer program deployed successfully!"

# echo -e "\n[+] starting coordinator deploy"
# pushd architectures/decentralized/solana-coordinator
# echo -e "\n[+] syncing keys..."
# anchor keys sync --provider.cluster ${RPC} --provider.wallet $WALLET_FILE

# echo -e "\n[+] building..."
# anchor build --no-idl

# echo -e "\n[+] deploying..."
# anchor deploy --provider.cluster ${RPC} --provider.wallet $WALLET_FILE -- --max-len 500000
# popd
# echo -e "\n[+] Coordinator program deployed successfully!"

# sleep 10

# echo -e "\n[+] Creating training run..."
# cargo run --release --bin psyche-solana-client -- \
    #         create-run \
    #         --wallet-private-key-path ${WALLET_FILE} \
    #         --rpc ${RPC} \
    #         --ws-rpc ${WS_RPC} \
    #         --join-authority HaWerxxEuQm1437bs79aGAhH31wg8ztAPrSwx412yDNf \
    #         --run-id ${RUN_ID} "$@"
# echo -e "\n[+] Training run created successfully"

# cargo run --release --bin psyche-solana-client -- \
    #         set-future-epoch-rates \
    #         --wallet-private-key-path ${WALLET_FILE} \
    #         --rpc ${RPC} \
    #         --ws-rpc ${WS_RPC} \
    #         --run-id ${RUN_ID} \
    #         --earning-rate 1

# cargo run --release --features parallelism --bin psyche-solana-client -- \
    #             tick \
    #             --wallet-private-key-path ${WALLET_FILE} \
    #             --rpc ${RPC} \
    #             --ws-rpc ${WS_RPC} \
    #             --run-id ${RUN_ID}


# cargo run --release --features parallelism --bin psyche-solana-client -- \
    #        set-paused \
    #        --wallet-private-key-path ${WALLET_FILE} \
    #        --rpc ${RPC} \
    #        --ws-rpc ${WS_RPC} \
    #        --run-id ${RUN_ID}

# cargo run --release --features parallelism --bin psyche-solana-client -- \
    #         update-config \
    #         --wallet-private-key-path ${WALLET_FILE} \
    #         --rpc ${RPC} \
    #         --ws-rpc ${WS_RPC} \
    #         --run-id ${RUN_ID} \
    #         --config-path ${CONFIG_FILE} \
    #         --restart-from-step 53

cargo run --release --features parallelism --bin psyche-solana-client -- \
    set-paused \
    --wallet-private-key-path ${WALLET_FILE} \
    --rpc ${RPC} \
    --ws-rpc ${WS_RPC} \
    --run-id ${RUN_ID} \
    --resume

# cargo run --release --bin psyche-solana-client -- \
    #         show progress \
    #         --rpc ${RPC} \
    #         --ws-rpc ${WS_RPC} \
    #         --run-id ${RUN_ID}

# cargo run --release --features parallelism --bin psyche-solana-client -- \
    #             tick \
    #             --wallet-private-key-path ${WALLET_FILE} \
    #             --rpc ${RPC} \
    #             --ws-rpc ${WS_RPC} \
    #             --run-id ${RUN_ID}

# cargo run --release --bin psyche-solana-client -- \
    #     checkpoint \
    #     --wallet-private-key-path ${WALLET_FILE} \
    #     --rpc ${RPC} \
    #     --ws-rpc ${WS_RPC} \
    #     --run-id ${RUN_ID} \
    #     --repo PsycheFoundation/consilience-40b-CqX3FUm4 \
    #     --revision a3aa8f7565da8ff6d33968c93c8fa5d38f5ef128

# cargo run --release --bin psyche-solana-client -- \
    #     show model \
    #     --rpc ${RPC} \
    #     --ws-rpc ${WS_RPC} \
    #     --run-id ${RUN_ID}