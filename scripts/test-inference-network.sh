#!/usr/bin/env bash
set -euo pipefail

# Test inference network with 2 nodes in tmux

SESSION_NAME="inference-test"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Building test-network binary...${NC}"
cargo build --bin test-network

echo -e "${GREEN}✓ Build complete${NC}"

# Kill existing session if it exists
tmux kill-session -t "$SESSION_NAME" 2>/dev/null || true

echo -e "${BLUE}Starting tmux session: $SESSION_NAME${NC}"

# Create new session with first node
tmux new-session -d -s "$SESSION_NAME" -n "node1"
tmux send-keys -t "$SESSION_NAME:node1" "cargo run --bin test-network -- --node-id node1" C-m

# Create second window for second node
sleep 1
tmux new-window -t "$SESSION_NAME" -n "node2"
tmux send-keys -t "$SESSION_NAME:node2" "cargo run --bin test-network -- --node-id node2" C-m

# Create third window for logs/instructions
tmux new-window -t "$SESSION_NAME" -n "info"
tmux send-keys -t "$SESSION_NAME:info" "cat << 'EOF'
Inference Network Test
======================

Windows:
  node1  - First test node
  node2  - Second test node
  info   - This window (instructions)

What to look for:
  - Both nodes should print their Endpoint IDs
  - Each node should see \"PEER DISCOVERED!\" message
  - Peer details should show test-model-node1 and test-model-node2

Commands:
  Ctrl+B, 1  - Switch to node1
  Ctrl+B, 2  - Switch to node2
  Ctrl+B, 3  - Switch to info
  Ctrl+C     - Stop a node (in its window)

To exit test:
  Type 'exit' in each window or run: tmux kill-session -t $SESSION_NAME

Logs are live - watch for PEER DISCOVERED messages!
EOF
" C-m

echo -e "${GREEN}✓ Test started!${NC}"
echo -e "${BLUE}Attaching to tmux session...${NC}"
echo ""
echo "Use 'Ctrl+B, d' to detach from tmux"
echo "Use 'tmux attach -t $SESSION_NAME' to reattach"
echo "Use 'tmux kill-session -t $SESSION_NAME' to stop all nodes"
echo ""

# Attach to the session
tmux attach -t "$SESSION_NAME"
