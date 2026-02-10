#!/usr/bin/env bash
# End-to-end test for inference network: starts inference node + gateway, sends test request
# Note: Must be run from nix shell: nix develop .#dev-python --command ./scripts/test-inference-e2e.sh

set -euo pipefail

MODEL="${1:-gpt2}"
PROMPT="${2:-Hello, world! This is a test.}"

echo "Starting inference network test..."
echo "  Model: $MODEL"
echo "  Prompt: $PROMPT"
echo ""

# Cleanup function
cleanup() {
    echo ""
    echo "Cleaning up..."
    if [ ! -z "${INFERENCE_PID:-}" ]; then
        kill $INFERENCE_PID 2>/dev/null || true
    fi
    if [ ! -z "${GATEWAY_PID:-}" ]; then
        kill $GATEWAY_PID 2>/dev/null || true
    fi
}
trap cleanup EXIT

# Start inference node
echo "Starting inference node..."
RUST_LOG=info nix run .#psyche-inference-node -- \
    --model-name "$MODEL" \
    --discovery-mode local \
    > /tmp/psyche-inference-node.log 2>&1 &
INFERENCE_PID=$!

echo "  Inference node PID: $INFERENCE_PID"
echo "  Waiting for node to initialize..."
sleep 5

# Check if inference node is still running
if ! kill -0 $INFERENCE_PID 2>/dev/null; then
    echo "Inference node failed to start. Check /tmp/psyche-inference-node.log"
    tail -20 /tmp/psyche-inference-node.log
    exit 1
fi

# Start gateway
echo "Starting gateway node..."
RUST_LOG=info nix run .#bin-psyche-inference-node-gateway-node -- \
    --discovery-mode local \
    > /tmp/psyche-gateway-node.log 2>&1 &
GATEWAY_PID=$!

echo "  Gateway node PID: $GATEWAY_PID"
echo "  Waiting for gateway to initialize..."
sleep 3

# Check if gateway is still running
if ! kill -0 $GATEWAY_PID 2>/dev/null; then
    echo "Gateway node failed to start. Check /tmp/psyche-gateway-node.log"
    tail -20 /tmp/psyche-gateway-node.log
    exit 1
fi

echo ""
echo "Both nodes running!"
echo ""
echo "Sending test inference request..."

# Send test request
RESPONSE=$(curl -s -X POST http://127.0.0.1:8000/v1/chat/completions \
        -H "Content-Type: application/json" \
        -d "{\"messages\": [{\"role\": \"user\", \"content\": \"$PROMPT\"}], \"max_tokens\": 50}" \
    -w "\nHTTP_STATUS:%{http_code}")

HTTP_STATUS=$(echo "$RESPONSE" | grep "HTTP_STATUS:" | cut -d: -f2)
BODY=$(echo "$RESPONSE" | grep -v "HTTP_STATUS:")

echo ""
if [ "$HTTP_STATUS" = "200" ]; then
    echo "Request succeeded (HTTP $HTTP_STATUS)"
    echo ""
    echo "Response:"
    echo "$BODY" | jq . 2>/dev/null || echo "$BODY"
else
    echo "Request failed (HTTP $HTTP_STATUS)"
    echo ""
    echo "Response:"
    echo "$BODY"
    echo ""
    echo "Inference node log:"
    tail -20 /tmp/psyche-inference-node.log
    echo ""
    echo "Gateway node log:"
    tail -20 /tmp/psyche-gateway-node.log
    exit 1
fi

echo ""
echo "Test completed successfully!"
