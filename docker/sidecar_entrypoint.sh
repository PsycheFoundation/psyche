#!/bin/bash

# set -o errexit
# set -euo pipefail

# Sanity checks
if [[ "$PSYCHE_MAIN_HOST" == "" ]]; then
    echo -e "\n[!] The PSYCHE_MAIN_HOST env variable was not set."
    exit 1
fi

if [[ "$PSYCHE_WORLD_SIZE" == "" ]]; then
    echo -e "\n[!] The PSYCHE_WORLD_SIZE env variable was not set."
    exit 1
fi

if [[ "$PSYCHE_RANK" == "" ]]; then
    echo -e "\n[!] The PSYCHE_RANK env variable was not set."
    exit 1
fi

IMPL=${PSYCHE_IMPL:-python}

echo "
Multi-Node Psyche Sidecar
========================
Main Host:      $PSYCHE_MAIN_HOST
World Size:     $PSYCHE_WORLD_SIZE
Rank:           $PSYCHE_RANK
Implementation: $IMPL
"

PID=0

handle_signal() {
    echo "Received signal, stopping..."
    if [[ $PID -ne 0 ]]; then
        kill -TERM "$PID" 2>/dev/null || true
        wait "$PID"
    fi
    exit 0
}

# Trap SIGTERM and SIGINT
trap handle_signal TERM INT

# after this time period, reset the restart counter
RESET_TIME=60
MAX_RESTARTS=5
num_restarts=0

while true; do
    echo -e "\n[+] Starting $IMPL sidecar (rank $PSYCHE_RANK)..."

    start_time=$SECONDS

    /bin/psyche-sidecar $IMPL \
        --main-host $PSYCHE_MAIN_HOST \
        --world-size $PSYCHE_WORLD_SIZE \
        --rank $PSYCHE_RANK &

    PID=$!
    wait "$PID" || true

    duration=$((SECONDS - start_time))
    EXIT_STATUS=$?
    echo -e "\n[!] Sidecar exited with status '$EXIT_STATUS'."

    PID=0

    if [ $duration -ge $RESET_TIME ]; then
        num_restarts=0
        echo "Sidecar ran for >${RESET_TIME} seconds, resetting restart counter"
    else
        ((num_restarts += 1))
    fi

    if [[ $num_restarts -ge $MAX_RESTARTS ]]; then
        echo -e "[!] Maximum restarts ($num_restarts) reached. Exiting..."
        exit 1
    fi

    echo "Waiting 5 seconds before restart..."
    sleep 5
done
