#! /bin/bash
set -o errexit
solana config set --url "${RPC}"
solana-keygen new --no-bip39-passphrase --force
solana airdrop 10 "$(solana-keygen pubkey)"
echo "Python enabled ${PYTHON_ENABLED}"
echo "EVAL_TASKS=${EVAL_TASKS}"
echo "EVAL_TASK_MAX_DOCS=${EVAL_TASK_MAX_DOCS}"
echo "DATA_PARALLELISM=${DATA_PARALLELISM}"

SIDECAR_PORT=$(shuf -i 9000-9100 -n 1)
echo "USING SIDECAR PORT: ${SIDECAR_PORT}"

# Build the command based on environment variable
if [ "${PYTHON_ENABLED}" = "true" ]; then
    echo "Starting client with Python features enabled"
    CMD="psyche-solana-client train \
        --wallet-private-key-path /root/.config/solana/id.json \
        --rpc ${RPC} \
        --ws-rpc ${WS_RPC} \
        --run-id ${RUN_ID} \
        --data-parallelism 2 \
        --sidecar-port ${SIDECAR_PORT} \
        --iroh-relay n0 \
        --logs json"

    # Add eval tasks if configured
    if [ -n "${EVAL_TASKS}" ]; then
        echo "Enabling evaluations: ${EVAL_TASKS}"
        CMD="${CMD} --eval-tasks ${EVAL_TASKS}"
    fi

    if [ -n "${EVAL_TASK_MAX_DOCS}" ]; then
        CMD="${CMD} --eval-task-max-docs ${EVAL_TASK_MAX_DOCS}"
    fi

    echo "Final command: ${CMD}"
    eval "${CMD}"
else
    echo "Starting client without Python features"
    psyche-solana-client train \
        --wallet-private-key-path "/root/.config/solana/id.json" \
        --rpc "${RPC}" \
        --ws-rpc "${WS_RPC}" \
        --run-id "${RUN_ID}" \
        --logs "json"
fi
