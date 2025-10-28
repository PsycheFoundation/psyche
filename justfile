mod nix

default:
    just --list

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

# Deploy coordinator on localnet and create a "test" run for 1.1b model.
setup-solana-localnet-test-run run_id="test" *args='':
    RUN_ID={{ run_id }} ./scripts/setup-and-deploy-solana-test.sh {{ args }}

# Deploy coordinator on localnet and create a "test" run for 20m model.
setup-solana-localnet-light-test-run run_id="test" *args='':
    RUN_ID={{ run_id }} CONFIG_FILE=./config/solana-test/light-config.toml ./scripts/setup-and-deploy-solana-test.sh {{ args }}

# Start client for training on localnet.
start-training-localnet-client run_id="test" *args='':
    RUN_ID={{ run_id }} ./scripts/train-solana-test.sh {{ args }}

# Start client for training on localnet without data parallelism features and using light model.
start-training-localnet-light-client run_id="test" *args='':
    RUN_ID={{ run_id }} BATCH_SIZE=1 DP=1 ./scripts/train-solana-test.sh {{ args }}

OTLP_METRICS_URL := "http://localhost:4318/v1/metrics"
OTLP_LOGS_URL := "http://localhost:4318/v1/logs"

# The same command as above but with arguments set to export telemetry data
start-training-localnet-light-client-telemetry run_id="test" *args='':
    OTLP_METRICS_URL={{ OTLP_METRICS_URL }} OTLP_LOGS_URL={{ OTLP_LOGS_URL }} RUN_ID={{ run_id }} BATCH_SIZE=1 DP=1 ./scripts/train-solana-test.sh {{ args }}

DEVNET_RPC := "https://api.devnet.solana.com"
DEVNET_WS_RPC := "wss://api.devnet.solana.com"

# Deploy coordinator on Devnet and create a "test" run for 1.1b model.
setup-solana-devnet-test-run run_id="test" *args='':
    RUN_ID={{ run_id }} RPC={{ DEVNET_RPC }} WS_RPC={{ DEVNET_WS_RPC }} ./scripts/deploy-solana-test.sh {{ args }}

# Deploy coordinator on Devnet and create a "test" run for 20m model.
setup-solana-devnet-light-test-run run_id="test" *args='':
    RUN_ID={{ run_id }} RPC={{ DEVNET_RPC }} WS_RPC={{ DEVNET_WS_RPC }} CONFIG_FILE=./config/solana-test/light-config.toml ./scripts/deploy-solana-test.sh  {{ args }}

# Start client for training on Devnet.
start-training-devnet-client run_id="test" *args='':
    RUN_ID={{ run_id }} RPC={{ DEVNET_RPC }} WS_RPC={{ DEVNET_WS_RPC }} ./scripts/train-solana-test.sh {{ args }}

# Start client for training on localnet without data parallelism features and using light model.
start-training-devnet-light-client run_id="test" *args='':
    RUN_ID={{ run_id }} RPC={{ DEVNET_RPC }} WS_RPC={{ DEVNET_WS_RPC }} BATCH_SIZE=1 DP=1 ./scripts/train-solana-test.sh {{ args }}

solana-client-tests:
    cargo test --package psyche-solana-client --features solana-localnet-tests

# install deps for building mdbook
book_deps:
    cargo install mdbook mdbook-mermaid mdbook-linkcheck

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

run_docker_client *ARGS:
    just nix build_docker_solana_client
    docker run -d {{ ARGS }} --gpus all psyche-prod-solana-client

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
    cd docker/test
    if [ "${USE_GPU}" != "0" ] && command -v nvidia-smi &> /dev/null; then
        echo "GPU detected and USE_GPU not set to 0, enabling GPU support"
        NUM_REPLICAS={{ num_clients }} docker compose -f docker-compose.yml -f docker-compose.gpu.yml up -d --force-recreate
    else
        echo "Running without GPU support"
        NUM_REPLICAS={{ num_clients }} docker compose -f docker-compose.yml up -d --force-recreate
    fi

run_test_infra_with_proxies_validator num_clients="1":
    #!/usr/bin/env bash
    if [ "${USE_GPU}" != "0" ] && command -v nvidia-smi &> /dev/null; then
        echo "GPU detected and USE_GPU not set to 0, enabling GPU support"
        cd docker/test/subscriptions_test && NUM_REPLICAS={{ num_clients }} docker compose -f ../docker-compose.yml -f docker-compose.yml -f ../docker-compose.gpu.yml up -d --force-recreate
    else
        echo "Running without GPU support"
        cd docker/test/subscriptions_test && NUM_REPLICAS={{ num_clients }} docker compose -f ../docker-compose.yml -f docker-compose.yml up -d --force-recreate
    fi

stop_test_infra:
    cd docker/test && docker compose -f docker-compose.yml -f subscriptions_test/docker-compose.yml down
