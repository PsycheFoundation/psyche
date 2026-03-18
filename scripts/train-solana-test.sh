AUTHORIZER=${AUTHORIZER:-"11111111111111111111111111111111"}

# presets for a DGX or an HGX
DP=${DP:-"8"}
TP=${TP:-"1"}
BATCH_SIZE=${BATCH_SIZE:-"1"}

# fine if this fails
solana airdrop 10 "$(solana-keygen pubkey ${WALLET_FILE})" --url "${RPC}" || true
export RUST_LOG="info,psyche=debug"

if [[ "$OTLP_METRICS_URL" == "" ]]; then
    cargo run --release --bin psyche-solana-client -- \
        train \
        --wallet-private-key-path ${WALLET_FILE} \
        --rpc ${RPC} \
        --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} \
        --data-parallelism ${DP} \
        --tensor-parallelism ${TP} \
        --micro-batch-size ${BATCH_SIZE} \
        --authorizer ${AUTHORIZER} \
        --logs "console" \
        "$@"
else
    cargo run --release --bin psyche-solana-client -- \
        train \
        --wallet-private-key-path ${WALLET_FILE} \
        --rpc ${RPC} \
        --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} \
        --data-parallelism ${DP} \
        --tensor-parallelism ${TP} \
        --micro-batch-size ${BATCH_SIZE} \
        --logs "console" \
        --authorizer ${AUTHORIZER} \
        --oltp-metrics-url "http://localhost:4318/v1/metrics" \
        --oltp-logs-url "http://localhost:4318/v1/logs" \
        "$@"
fi
