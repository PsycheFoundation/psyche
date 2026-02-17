mod nix
mod dev 'architectures/decentralized/justfile'

default:
    just --list

check-client:
    cargo run -p psyche-solana-client -- --help

# test inference network discovery (2 nodes in tmux)
test-inference-network:
    ./scripts/test-inference-network.sh

# format & lint-fix code
fmt:
    echo "deprecated, use 'nix fmt' instead..."
    sleep 5
    cargo clippy --fix --allow-staged --all-targets
    cargo fmt
    nixfmt .

# spin up a local testnet
local-testnet *args='':
    OLTP_METRICS_URL="http://localhost:4318/v1/metrics" OLTP_TRACING_URL="http://localhost:4318/v1/traces" OLTP_LOGS_URL="http://localhost:4318/v1/logs" cargo run -p psyche-centralized-local-testnet -- start {{ args }}

# run integration tests
integration-test test_name="":
    if [ "{{ test_name }}" = "" ]; then \
        cargo test --release -p psyche-centralized-testing --test integration_tests; \
    else \
        cargo test --release -p psyche-centralized-testing --test integration_tests -- --nocapture "{{ test_name }}"; \
    fi

# Determine whether to use Python support based on environment variable

use_python := env("USE_PYTHON", "0")

# Run decentralized integration tests with optional Python support and test filtering
decentralized-integration-tests test_name="":
    #!/usr/bin/env bash
    set -euo pipefail

    if [[ "{{ use_python }}" == "1" ]]; then
        echo "Running tests with Python support"
        just setup_python_test_infra

        if [[ -z "{{ test_name }}" ]]; then
            cargo test --release \
                -p psyche-decentralized-testing \
                --features python,parallelism \
                --test integration_tests \
                -- --nocapture
        else
            cargo test --release \
                -p psyche-decentralized-testing \
                --features python,parallelism \
                --test integration_tests \
                -- --nocapture "{{ test_name }}"
        fi
    else
        echo "Running tests without Python support"
        just setup_test_infra

        if [[ -z "{{ test_name }}" ]]; then
            cargo test --release \
                -p psyche-decentralized-testing \
                --test integration_tests \
                -- --nocapture
        else
            cargo test --release \
                -p psyche-decentralized-testing \
                --test integration_tests \
                -- --nocapture "{{ test_name }}"
        fi
    fi

# run integration decentralized chaos tests
decentralized-chaos-integration-test test_name="":
    if [ "{{ test_name }}" = "" ]; then \
        cargo test --release -p psyche-decentralized-testing --test chaos_tests -- --nocapture; \
    else \
        cargo test --release -p psyche-decentralized-testing --test chaos_tests -- --nocapture "{{ test_name }}"; \
    fi

solana-client-tests:
    cargo test --package psyche-solana-client --features solana-localnet-tests

build_book output-dir="../book": generate_cli_docs
    mdbook build psyche-book -d {{ output-dir }}

# run an interactive development server for psyche-book
serve_book: generate_cli_docs
    mdbook serve psyche-book --open

generate_cli_docs:
    echo "generating CLI --help outputs for mdbook..."
    mkdir -p psyche-book/generated/cli/
    cargo run -p psyche-centralized-client print-all-help --markdown > psyche-book/generated/cli/psyche-centralized-client.md
    cargo run -p psyche-centralized-server print-all-help --markdown > psyche-book/generated/cli/psyche-centralized-server.md
    cargo run -p psyche-centralized-local-testnet print-all-help --markdown > psyche-book/generated/cli/psyche-centralized-local-testnet.md
    cargo run -p psyche-sidecar print-all-help --markdown > psyche-book/generated/cli/psyche-sidecar.md
    cargo run -p psyche-solana-client print-all-help --markdown > psyche-book/generated/cli/psyche-solana-client.md

run_docker_client *ARGS:
    just nix build_docker_solana_client
    docker run -d {{ ARGS }} --gpus all psyche-solana-client

# Setup clients assigning one available GPU to each of them.

# There's no way to do this using the replicas from docker compose file, so we have to do it manually.
setup_gpu_clients num_clients="1":
    ./scripts/coordinator-address-check.sh
    just nix build_docker_solana_test_client
    ./scripts/train-multiple-gpu-localnet.sh {{ num_clients }}

clean_stale_images:
    docker rmi $(docker images -f dangling=true -q)

# Build & push the centralized client Docker image
docker_push_centralized_client:
    just nix docker_build_centralized_client
    docker push docker.io/nousresearch/psyche-centralized-client

# Setup the infrastructure for testing locally using Docker.
setup_test_infra:
    cd architectures/decentralized/solana-coordinator && anchor build
    cd architectures/decentralized/solana-authorizer && anchor build
    just nix build_docker_solana_test_client_no_python
    just nix build_docker_solana_test_validator

# Setup the infrastructure for testing locally using Docker.
setup_python_test_infra:
    cd architectures/decentralized/solana-coordinator && anchor build
    cd architectures/decentralized/solana-authorizer && anchor build
    just nix build_docker_solana_test_client
    just nix build_docker_solana_test_validator

run_test_infra num_clients="1":
    #!/usr/bin/env bash
    set -e

    cd docker/test

    # Start validator only first
    echo "Starting validator and deploying contracts..."
    docker compose up -d --wait psyche-solana-test-validator

    sleep 2  # Extra buffer for RPC to be fully ready

    # Run setup script from project root
    echo "Setting up test run..."
    cd ../..
    ./scripts/setup-test-run.sh

    # Now start the client services
    cd docker/test
    echo "Starting clients..."
    if [ "${USE_GPU}" != "0" ] && command -v nvidia-smi &> /dev/null; then
        echo "GPU detected and USE_GPU not set to 0, enabling GPU support"
        NUM_REPLICAS={{ num_clients }} docker compose -f docker-compose.yml -f docker-compose.gpu.yml up -d psyche-test-client
    else
        echo "Running without GPU support"
        NUM_REPLICAS={{ num_clients }} docker compose -f docker-compose.yml up -d psyche-test-client
    fi

run_test_infra_with_rpc_fallback_proxies num_clients="1":
    #!/usr/bin/env bash
    set -e

    cd docker/test/rpc_fallback_test

    # Start validator only first
    echo "Starting validator and deploying contracts..."
    docker compose -f ../docker-compose.yml up -d --wait psyche-solana-test-validator

    sleep 2  # Extra buffer for RPC to be fully ready

    # Run setup script from project root
    echo "Setting up test run..."
    cd ../../..
    RPC="http://127.0.0.1:8899" WS_RPC="ws://127.0.0.1:8900" RUN_ID="test" ./scripts/setup-test-run.sh

    # Now start the client and proxy services
    cd docker/test/rpc_fallback_test
    echo "Starting clients and proxies..."
    if [ "${USE_GPU}" != "0" ] && command -v nvidia-smi &> /dev/null; then
        echo "GPU detected and USE_GPU not set to 0, enabling GPU support"
        NUM_REPLICAS={{ num_clients }} docker compose -f ../docker-compose.yml -f docker-compose.yml -f ../docker-compose.gpu.yml up -d psyche-test-client nginx nginx_2
    else
        echo "Running without GPU support"
        NUM_REPLICAS={{ num_clients }} docker compose -f ../docker-compose.yml -f docker-compose.yml up -d psyche-test-client nginx nginx_2
    fi

stop_test_infra:
    cd docker/test && docker compose -f docker-compose.yml -f rpc_fallback_test/docker-compose.yml down

# Run inference node with a local model (requires Python venv with vLLM)
inference-node model="gpt2":
    RUST_LOG=info,psyche_network=debug nix run .#psyche-inference-node -- \
        --model-name {{ model }} \
        --discovery-mode n0 \
        --relay-kind n0

# Run gateway node (HTTP API for inference requests)
gateway-node:
    RUST_LOG=info,psyche_network=debug nix run .#bin-psyche-inference-node-gateway-node -- \
        --discovery-mode n0 \
        --relay-kind n0

# Run full inference stack (gateway + inference node in tmux)
inference-stack model="gpt2":
    #!/usr/bin/env bash
    set -euo pipefail

    # Check if tmux is available
    if ! command -v tmux &> /dev/null; then
        echo "Error: tmux is required but not installed"
        exit 1
    fi

    SESSION="psyche-inference"
    GATEWAY_PEER_FILE="/tmp/psyche-gateway-peer.json"

    # Clean up old peer file
    rm -f "$GATEWAY_PEER_FILE"

    # Kill existing session if it exists
    tmux kill-session -t $SESSION 2>/dev/null || true

    echo "building gateway and inference node..."
    nix build .#bin-psyche-inference-node-gateway-node .#psyche-inference-node

    echo "Starting gateway node (bootstrap node)..."

    # Create new session with gateway (starts first to be bootstrap node)
    tmux new-session -d -s $SESSION -n gateway
    tmux send-keys -t $SESSION:gateway "PSYCHE_GATEWAY_ENDPOINT_FILE=$GATEWAY_PEER_FILE RUST_LOG=info,psyche_network=debug nix run .#bin-psyche-inference-node-gateway-node -- --discovery-mode n0 --relay-kind n0" C-m

    # Wait for gateway to start and write peer file
    echo "Waiting for gateway to initialize and write endpoint..."
    for i in $(seq 1 30); do
        if [ -f "$GATEWAY_PEER_FILE" ]; then
            echo "Gateway peer file created"
            break
        fi
        sleep 1
    done

    if [ ! -f "$GATEWAY_PEER_FILE" ]; then
        echo "Error: Gateway failed to create peer file"
        exit 1
    fi

    # Wait a bit more for gateway HTTP server
    sleep 2
    echo "Gateway ready"
    echo ""
    echo "Starting inference node..."

    # Create window for inference node (bootstraps from gateway)
    tmux new-window -t $SESSION -n inference
    tmux send-keys -t $SESSION:inference "PSYCHE_GATEWAY_BOOTSTRAP_FILE=$GATEWAY_PEER_FILE RUST_LOG=info,psyche_network=debug nix run .#psyche-inference-node -- --model-name {{ model }} --discovery-mode n0 --relay-kind n0" C-m

    # Wait for inference node to start
    sleep 3
    echo "Inference node started"
    echo ""

    # Create window for testing
    tmux new-window -t $SESSION -n test
    tmux send-keys -t $SESSION:test "echo 'Test inference with:'; echo 'curl -X POST http://127.0.0.1:8000/v1/chat/completions -H \"Content-Type: application/json\" -d '\"'\"'{\"messages\": [{\"role\": \"user\", \"content\": \"Hello, world!\"}], \"max_tokens\": 50}'\"'\"''" C-m

    # Attach to session
    echo "Starting inference stack in tmux session '$SESSION'"
    echo "Windows: inference (node), gateway (HTTP API), test (for curl commands)"
    echo ""
    echo "To attach: tmux attach -t $SESSION"
    echo "To kill: tmux kill-session -t $SESSION"
    echo ""
    tmux attach -t $SESSION

# Test inference via HTTP (requires inference stack to be running)
test-inference prompt="Hello, world!" max_tokens="50":
    curl -X POST http://127.0.0.1:8000/v1/chat/completions \
        -H "Content-Type: application/json" \
        -d '{"messages": [{"role": "user", "content": "{{ prompt }}"}], "max_tokens": {{ max_tokens }}}'

# Run end-to-end test: start nodes, send request, verify response
test-inference-e2e model="gpt2" prompt="Hello, world!":
    ./scripts/test-inference-e2e.sh "{{ model }}" "{{ prompt }}"

# Test model assignment system with multiple nodes and models (gateway + 3 inference nodes)
test-model-assignment:
    #!/usr/bin/env bash
    set -euo pipefail

    # Check if tmux is available
    if ! command -v tmux &> /dev/null; then
        echo "Error: tmux is required but not installed"
        exit 1
    fi

    SESSION="psyche-model-assignment"
    GATEWAY_PEER_FILE="/tmp/psyche-gateway-peer.json"

    # Clean up old peer file
    rm -f "$GATEWAY_PEER_FILE"

    # Kill existing session if it exists
    tmux kill-session -t $SESSION 2>/dev/null || true

    echo "Building gateway and inference node..."
    nix build .#bin-psyche-inference-node-gateway-node .#psyche-inference-node

    echo "Starting gateway node (bootstrap node)..."

    # Create new session with gateway
    tmux new-session -d -s $SESSION -n gateway
    tmux send-keys -t $SESSION:gateway "PSYCHE_GATEWAY_ENDPOINT_FILE=$GATEWAY_PEER_FILE RUST_LOG=info,psyche_network=debug nix run .#bin-psyche-inference-node-gateway-node -- --discovery-mode local --relay-kind n0" C-m

    # Wait for gateway to start
    echo "Waiting for gateway to initialize..."
    for i in $(seq 1 30); do
        if [ -f "$GATEWAY_PEER_FILE" ]; then
            echo "Gateway peer file created"
            break
        fi
        sleep 1
    done

    if [ ! -f "$GATEWAY_PEER_FILE" ]; then
        echo "Error: Gateway failed to create peer file"
        exit 1
    fi

    sleep 2
    echo "Gateway ready"

    # Start inference node 1 in idle mode
    echo "Starting inference node 1 (idle mode)..."
    tmux new-window -t $SESSION -n node1
    tmux send-keys -t $SESSION:node1 "PSYCHE_GATEWAY_BOOTSTRAP_FILE=$GATEWAY_PEER_FILE RUST_LOG=info,psyche_network=debug nix run .#psyche-inference-node -- --discovery-mode local --relay-kind n0 --tensor-parallel-size 1 --gpu-memory-utilization 0.5" C-m

    # Start inference node 2 in idle mode
    echo "Starting inference node 2 (idle mode)..."
    tmux new-window -t $SESSION -n node2
    tmux send-keys -t $SESSION:node2 "PSYCHE_GATEWAY_BOOTSTRAP_FILE=$GATEWAY_PEER_FILE RUST_LOG=info,psyche_network=debug nix run .#psyche-inference-node -- --discovery-mode local --relay-kind n0 --tensor-parallel-size 1 --gpu-memory-utilization 0.35" C-m

    # Start inference node 3 in idle mode
    echo "Starting inference node 3 (idle mode)..."
    tmux new-window -t $SESSION -n node3
    tmux send-keys -t $SESSION:node3 "PSYCHE_GATEWAY_BOOTSTRAP_FILE=$GATEWAY_PEER_FILE RUST_LOG=info,psyche_network=debug nix run .#psyche-inference-node -- --discovery-mode local --relay-kind n0 --tensor-parallel-size 1 --gpu-memory-utilization 0.5" C-m

    sleep 5
    echo ""
    echo "All nodes started"
    echo ""

    # Create test window with instructions
    tmux new-window -t $SESSION -n test
    tmux send-keys -t $SESSION:test "cat << 'EOF'" C-m
    tmux send-keys -t $SESSION:test "═══════════════════════════════════════════════════════════════" C-m
    tmux send-keys -t $SESSION:test "  Model Assignment System Test" C-m
    tmux send-keys -t $SESSION:test "═══════════════════════════════════════════════════════════════" C-m
    tmux send-keys -t $SESSION:test "" C-m
    tmux send-keys -t $SESSION:test "Status:" C-m
    tmux send-keys -t $SESSION:test "  • Gateway: running on http://127.0.0.1:8000" C-m
    tmux send-keys -t $SESSION:test "  • Node 1: idle (no model)" C-m
    tmux send-keys -t $SESSION:test "  • Node 2: idle (no model)" C-m
    tmux send-keys -t $SESSION:test "  • Node 3: idle (no model)" C-m
    tmux send-keys -t $SESSION:test "" C-m
    tmux send-keys -t $SESSION:test "Test 1: View current assignments" C-m
    tmux send-keys -t $SESSION:test "────────────────────────────────────────────────────────────────" C-m
    tmux send-keys -t $SESSION:test "curl http://127.0.0.1:8000/admin/assignments | jq" C-m
    tmux send-keys -t $SESSION:test "" C-m
    tmux send-keys -t $SESSION:test "Expected: Empty array (no assignments yet)" C-m
    tmux send-keys -t $SESSION:test "" C-m
    tmux send-keys -t $SESSION:test "Test 2: Assign models to nodes (2 nodes to gpt2, 1 to llama)" C-m
    tmux send-keys -t $SESSION:test "────────────────────────────────────────────────────────────────" C-m
    tmux send-keys -t $SESSION:test "curl -X POST http://127.0.0.1:8000/admin/assign-models \\\\" C-m
    tmux send-keys -t $SESSION:test "  -H 'Content-Type: application/json' \\\\" C-m
    tmux send-keys -t $SESSION:test "  -d '{" C-m
    tmux send-keys -t $SESSION:test "    \"assignments\": [" C-m
    tmux send-keys -t $SESSION:test "      {\"model_name\": \"gpt2\", \"num_nodes\": 2, \"source_type\": \"huggingface\"}," C-m
    tmux send-keys -t $SESSION:test "      {\"model_name\": \"meta-llama/Llama-3.2-1B-Instruct\", \"num_nodes\": 1, \"source_type\": \"huggingface\"}" C-m
    tmux send-keys -t $SESSION:test "    ]" C-m
    tmux send-keys -t $SESSION:test "  }'" C-m
    tmux send-keys -t $SESSION:test "" C-m
    tmux send-keys -t $SESSION:test "Expected: Gateway assigns 2 nodes to gpt2, 1 to llama" C-m
    tmux send-keys -t $SESSION:test "Watch node windows for LoadModel messages and loading progress" C-m
    tmux send-keys -t $SESSION:test "" C-m
    tmux send-keys -t $SESSION:test "Test 3: View assignments (wait ~10s for models to load)" C-m
    tmux send-keys -t $SESSION:test "────────────────────────────────────────────────────────────────" C-m
    tmux send-keys -t $SESSION:test "curl http://127.0.0.1:8000/admin/assignments | jq" C-m
    tmux send-keys -t $SESSION:test "" C-m
    tmux send-keys -t $SESSION:test "Expected: Shows node_id, model_name, status for each assignment" C-m
    tmux send-keys -t $SESSION:test "Status values: 'loading', 'loaded', 'idle', 'offline'" C-m
    tmux send-keys -t $SESSION:test "" C-m
    tmux send-keys -t $SESSION:test "Test 4: Send inference request to gpt2 nodes" C-m
    tmux send-keys -t $SESSION:test "────────────────────────────────────────────────────────────────" C-m
    tmux send-keys -t $SESSION:test "curl -X POST http://127.0.0.1:8000/v1/chat/completions \\\\" C-m
    tmux send-keys -t $SESSION:test "  -H 'Content-Type: application/json' \\\\" C-m
    tmux send-keys -t $SESSION:test "  -d '{\"model\": \"gpt2\", \"messages\": [{\"role\": \"user\", \"content\": \"Hello!\"}], \"max_tokens\": 50}'" C-m
    tmux send-keys -t $SESSION:test "" C-m
    tmux send-keys -t $SESSION:test "Test 5: Send inference request to llama node" C-m
    tmux send-keys -t $SESSION:test "────────────────────────────────────────────────────────────────" C-m
    tmux send-keys -t $SESSION:test "curl -X POST http://127.0.0.1:8000/v1/chat/completions \\\\" C-m
    tmux send-keys -t $SESSION:test "  -H 'Content-Type: application/json' \\\\" C-m
    tmux send-keys -t $SESSION:test "  -d '{\"model\": \"meta-llama/Llama-3.2-1B-Instruct\", \"messages\": [{\"role\": \"user\", \"content\": \"Hello!\"}], \"max_tokens\": 50}'" C-m
    tmux send-keys -t $SESSION:test "" C-m
    tmux send-keys -t $SESSION:test "Navigation:" C-m
    tmux send-keys -t $SESSION:test "  • Switch windows: Ctrl-b then 0/1/2/3/4" C-m
    tmux send-keys -t $SESSION:test "    0=gateway, 1=node1, 2=node2, 3=node3, 4=test" C-m
    tmux send-keys -t $SESSION:test "  • Exit tmux: Ctrl-b then d" C-m
    tmux send-keys -t $SESSION:test "  • Kill session: tmux kill-session -t psyche-model-assignment" C-m
    tmux send-keys -t $SESSION:test "═══════════════════════════════════════════════════════════════" C-m
    tmux send-keys -t $SESSION:test "EOF" C-m

    # Attach to session
    echo "Starting model assignment test in tmux session '$SESSION'"
    echo "Windows: gateway, node1, node2, node3, test"
    echo ""
    echo "To attach: tmux attach -t $SESSION"
    echo "To kill: tmux kill-session -t $SESSION"
    echo ""
    tmux attach -t $SESSION
