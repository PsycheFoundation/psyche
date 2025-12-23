#!/usr/bin/env bash
set -euo pipefail

# test inference network with 2 nodes in tmux

SESSION_NAME="inference-test"

GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${BLUE}Building test-network binary...${NC}"
cargo build --bin test-network

echo -e "${GREEN}Build complete${NC}"

ENDPOINT_FILE="/tmp/psyche-test-node1-endpoint.json"
rm -f "$ENDPOINT_FILE"

# check if we're already in a tmux session
if [ -n "${TMUX:-}" ]; then
    echo -e "${YELLOW}Already in tmux session. Using nested session.${NC}"
    NESTED=true
else
    NESTED=false
fi

tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true

echo -e "${BLUE}Starting tmux session: $SESSION_NAME${NC}"

if [ "$NESTED" = true ]; then
    # when nested, create detached session
    tmux -u new-session -d -s "$SESSION_NAME" -n "node1"
else
    tmux new-session -d -s "$SESSION_NAME" -n "node1"
fi

tmux send-keys -t "$SESSION_NAME:node1" "RUST_LOG=info,psyche_network=debug cargo run --bin test-network -- --node-id node1 --write-endpoint-file $ENDPOINT_FILE" C-m

echo -e "${BLUE}Waiting for node1 to initialize...${NC}"
for i in {1..10}; do
    if [ -f "$ENDPOINT_FILE" ]; then
        echo -e "${GREEN}Node1 endpoint ready${NC}"
        break
    fi
    sleep 1
done

echo -e "${BLUE}Waiting for node1 to stabilize...${NC}"
sleep 2

tmux new-window -t "$SESSION_NAME" -n "node2"
tmux send-keys -t "$SESSION_NAME:node2" "RUST_LOG=info,psyche_network=debug cargo run --bin test-network -- --node-id node2 --bootstrap-peer-file $ENDPOINT_FILE" C-m

tmux new-window -t "$SESSION_NAME" -n "info"
tmux send-keys -t "$SESSION_NAME:info" "cat << 'EOF'
Inference Network Test
======================

Windows:
  node1  - First test node
  node2  - Second test node
  info   - This window (instructions)

What to look for:
  - Node1 writes endpoint to file
  - Node2 reads endpoint and uses as bootstrap peer
  - Both nodes should print their Endpoint IDs
  - Node2 should see \"PEER DISCOVERED!\" message for node1
  - Node1 should see \"PEER DISCOVERED!\" message for node2
  - Peer details should show test-model-node1 and test-model-node2

Commands:
  Ctrl+B, 0  - Switch to node1
  Ctrl+B, 1  - Switch to node2
  Ctrl+B, 2  - Switch to info
  Ctrl+C     - Stop a node (in its window)

To exit test:
  Type 'exit' in each window or run: tmux kill-session -t $SESSION_NAME

Logs are live - watch for PEER DISCOVERED messages!
EOF
" C-m

echo -e "${GREEN}Test started!${NC}"

if [ "$NESTED" = true ]; then
    echo -e "${YELLOW}Nested tmux detected!${NC}"
    echo ""
    echo "To view the test session:"
    echo "  tmux switch-client -t $SESSION_NAME"
    echo ""
    echo "To switch back to your current session:"
    echo "  Ctrl+B, s  (then select your session)"
    echo ""
    echo "Or manually switch:"
    echo "  tmux switch-client -t <your-session-name>"
    echo ""
    echo "To stop all test nodes:"
    echo "  tmux kill-session -t $SESSION_NAME"
    echo ""

    tmux switch-client -t "$SESSION_NAME" 2>/dev/null || {
        echo -e "${YELLOW}Run manually: tmux attach -t $SESSION_NAME${NC}"
    }
else
    echo -e "${BLUE}Attaching to tmux session...${NC}"
    echo ""
    echo "Use 'Ctrl+B, d' to detach from tmux"
    echo "Use 'tmux attach -t $SESSION_NAME' to reattach"
    echo "Use 'tmux kill-session -t $SESSION_NAME' to stop all nodes"
    echo ""

    tmux attach -t "$SESSION_NAME"
fi
