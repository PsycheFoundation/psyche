#!/usr/bin/env bash
# Run the isolated network model sharing test.
#
# Sharer and downloaders run on separate Docker networks so they can only
# communicate through the relay (holepunching fails naturally).
#
# Usage:
#   ./run.sh                          # defaults: 10 params, 200MB, 5 downloaders
#   ./run.sh --params 20 --size 100   # custom
#   ./run.sh --downloaders 3

set -euo pipefail
cd "$(dirname "$0")"

NUM_PARAMS=10
PARAM_SIZE_MB=200
NUM_DOWNLOADERS=5
MAX_CONCURRENT=4
DISCOVERY_MODE=n0
RELAY_KIND=psyche

while [[ $# -gt 0 ]]; do
    case $1 in
        --params) NUM_PARAMS="$2"; shift 2 ;;
        --size) PARAM_SIZE_MB="$2"; shift 2 ;;
        --downloaders) NUM_DOWNLOADERS="$2"; shift 2 ;;
        --max-concurrent) MAX_CONCURRENT="$2"; shift 2 ;;
        --discovery) DISCOVERY_MODE="$2"; shift 2 ;;
        --relay) RELAY_KIND="$2"; shift 2 ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

LOG="/tmp/isolated_test_$(date +%s).log"

echo "=== Isolated Network Model Sharing Test ==="
echo "  Parameters:     $NUM_PARAMS x ${PARAM_SIZE_MB}MB"
echo "  Downloaders:    $NUM_DOWNLOADERS"
echo "  Max concurrent: $MAX_CONCURRENT"
echo "  Discovery:      $DISCOVERY_MODE"
echo "  Relay:          $RELAY_KIND"
echo "  Log file:       $LOG"
echo ""

export NUM_PARAMS PARAM_SIZE_MB NUM_DOWNLOADERS MAX_CONCURRENT DISCOVERY_MODE RELAY_KIND

echo "Building Docker image (first run will compile from source)..."
docker compose build 2>&1 | tee -a "$LOG"

echo ""
echo "Starting test (sharer on sharer-net, downloaders on downloader-net)..."
docker compose up --abort-on-container-exit 2>&1 | tee -a "$LOG"

echo ""
echo "Cleaning up..."
docker compose down -v

echo ""
echo "Done. Full log at: $LOG"
