#!/bin/bash

set -e

echo "Stopping all Docker containers..."
docker stop $(docker ps -q) 2>/dev/null || echo "No containers running"

echo "Cleaning Docker system..."
docker system prune -a -f

echo "Stopping Solana validator processes..."
pkill -f solana-test-validator || echo "No validator processes running"

echo "Setting up development environment in tmux..."

# tmux session name
SESSION_NAME="psyche-dev"

# Create new tmux session with first window
tmux new-session -d -s "$SESSION_NAME" -n "telemetry"

# Window 0: Docker Compose Telemetry
tmux send-keys -t "$SESSION_NAME:0" "cd /tmp/peter/psyche && docker compose -f telemetry/docker-compose.yml up" C-m

# Window 1: Setup Solana Localnet
tmux new-window -t "$SESSION_NAME:1" -n "solana-setup"
tmux send-keys -t "$SESSION_NAME:1" "cd /tmp/peter/psyche && nix develop -c bash -c 'just setup-solana-localnet-light-test-run'" C-m

echo "Waiting 60 seconds for Solana setup to complete..."
sleep 60

# Window 2: Client 1
tmux new-window -t "$SESSION_NAME:2" -n "client-1"
tmux send-keys -t "$SESSION_NAME:2" "cd /tmp/peter/psyche" C-m
tmux send-keys -t "$SESSION_NAME:2" "export OTLP_METRICS_URL=\"http://localhost:4318/v1/metrics\"" C-m
tmux send-keys -t "$SESSION_NAME:2" "export OTLP_LOGS_URL=\"http://localhost:4318/v1/logs\"" C-m
tmux send-keys -t "$SESSION_NAME:2" "nix develop -c bash -c 'just start-training-localnet-light-client-telemetry'" C-m

echo "Waiting 30 seconds before starting second client..."
sleep 30

# Window 3: Client 2
tmux new-window -t "$SESSION_NAME:3" -n "client-2"
tmux send-keys -t "$SESSION_NAME:3" "cd /tmp/peter/psyche" C-m
tmux send-keys -t "$SESSION_NAME:3" "export OTLP_METRICS_URL=\"http://localhost:4318/v1/metrics\"" C-m
tmux send-keys -t "$SESSION_NAME:3" "export OTLP_LOGS_URL=\"http://localhost:4318/v1/logs\"" C-m
tmux send-keys -t "$SESSION_NAME:3" "nix develop -c bash -c 'just start-training-localnet-light-client-telemetry'" C-m

# Return to first window
tmux select-window -t "$SESSION_NAME:0"

echo "Environment configured!"
echo "Run: tmux attach -t $SESSION_NAME"
echo ""
echo "Available windows:"
echo "  0: telemetry (docker-compose)"
echo "  1: solana-setup"
echo "  2: client-1"
echo "  3: client-2"
echo ""
echo "Navigate between windows with: Ctrl+b [0-3]"
echo "To exit without closing: Ctrl+b d"
