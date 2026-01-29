#!/usr/bin/env bash
#
# Psyche Daemon Management Script
# Manages psyche-solana-client as a systemd user service
#
# Usage:
#   psyche-daemon.sh install
#   psyche-daemon.sh start <run_id> <env_file>
#   psyche-daemon.sh stop <run_id>
#   psyche-daemon.sh status <run_id>
#   psyche-daemon.sh logs <run_id>

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
RUNTIME_DIR="${HOME}/.local/share/psyche"
SERVICE_NAME="psyche-client"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_systemd() {
    if ! command -v systemctl &> /dev/null; then
        log_error "systemctl not found. This script requires systemd."
        exit 1
    fi

    if ! systemctl --user status &> /dev/null; then
        log_error "systemd user session not available."
        exit 1
    fi
}

generate_start_script() {
    local run_id="${1}"
    local env_file="${2}"
    local start_script="${RUNTIME_DIR}/start-client-${run_id}.sh"

    mkdir -p "${RUNTIME_DIR}"

    # Convert env_file to absolute path
    if [[ ! "${env_file}" = /* ]]; then
        env_file="${PROJECT_ROOT}/${env_file}"
    fi

    cat > "${start_script}" << OUTER_EOF
#!/bin/bash
set -euo pipefail

# Load environment
set -a
source "${env_file}"
set +a

export RUST_LOG="\${RUST_LOG:-info,psyche=debug}"

# Build the command arguments
ARGS=(
    train
    --wallet-private-key-path "\${WALLET_PRIVATE_KEY_PATH:-\${devnet__keypair__wallet_PATH}}"
    --rpc "\${RPC:-http://127.0.0.1:8899}"
    --ws-rpc "\${WS_RPC:-ws://127.0.0.1:8900}"
    --run-id "${run_id}"
    --data-parallelism "\${DP:-1}"
    --tensor-parallelism "\${TP:-1}"
    --micro-batch-size "\${BATCH_SIZE:-1}"
    --authorizer "\${AUTHORIZER:-11111111111111111111111111111111}"
    --logs console
)

# Add extra args if specified
if [[ -n "\${EXTRA_ARGS:-}" ]]; then
    ARGS+=(\${EXTRA_ARGS})
fi

cd "${PROJECT_ROOT}"

# If PSYCHE_BINARY is set, use it directly
if [[ -n "\${PSYCHE_BINARY:-}" ]]; then
    exec "\${PSYCHE_BINARY}" "\${ARGS[@]}"
fi

# Check if we're in a nix environment (flake.nix exists)
if [[ -f "${PROJECT_ROOT}/flake.nix" ]]; then
    exec nix develop "${PROJECT_ROOT}" --command cargo run --release --bin psyche-solana-client -- "\${ARGS[@]}"
else
    exec cargo run --release --bin psyche-solana-client -- "\${ARGS[@]}"
fi
OUTER_EOF

    chmod +x "${start_script}"
}

cmd_install() {
    check_systemd

    log_info "Installing systemd service files..."

    mkdir -p "${SYSTEMD_USER_DIR}"
    mkdir -p "${RUNTIME_DIR}"

    # Create a simple service that runs our start script
    cat > "${SYSTEMD_USER_DIR}/${SERVICE_NAME}@.service" << EOF
[Unit]
Description=Psyche Training Client (%i)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/bin/bash ${RUNTIME_DIR}/start-client-%i.sh
Environment=RUST_LOG=info,psyche=debug
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
EOF

    systemctl --user daemon-reload
    log_info "Installed service to ${SYSTEMD_USER_DIR}/${SERVICE_NAME}@.service"

    # Enable lingering if possible
    if command -v loginctl &> /dev/null; then
        if ! loginctl show-user "$USER" 2>/dev/null | grep -q "Linger=yes"; then
            log_warn "Enabling lingering (may require sudo)..."
            sudo loginctl enable-linger "$USER" 2>/dev/null || \
                log_warn "Could not enable lingering. Services may stop after logout."
        fi
    fi

    log_info "Installation complete!"
}

cmd_start() {
    local run_id="${1:-}"
    local env_file="${2:-}"

    if [[ -z "${run_id}" ]] || [[ -z "${env_file}" ]]; then
        log_error "Usage: $0 start <run_id> <env_file>"
        echo "Example: $0 start test config/client/.env.local"
        exit 1
    fi

    check_systemd

    # Check env file exists
    local abs_env_file="${env_file}"
    if [[ ! "${env_file}" = /* ]]; then
        abs_env_file="${PROJECT_ROOT}/${env_file}"
    fi

    if [[ ! -f "${abs_env_file}" ]]; then
        log_error "Environment file not found: ${abs_env_file}"
        exit 1
    fi

    # Generate the start script with this env file
    generate_start_script "${run_id}" "${env_file}"
    log_info "Generated start script for run '${run_id}' using ${env_file}"

    log_info "Starting ${SERVICE_NAME}@${run_id}..."
    systemctl --user start "${SERVICE_NAME}@${run_id}.service"

    sleep 1
    if systemctl --user is-active --quiet "${SERVICE_NAME}@${run_id}.service"; then
        log_info "Service started successfully!"
        echo ""
        echo "View logs: $0 logs ${run_id}"
        echo "Stop:      $0 stop ${run_id}"
    else
        log_error "Service failed to start. Check logs:"
        journalctl --user -u "${SERVICE_NAME}@${run_id}.service" -n 20 --no-pager
    fi
}

cmd_stop() {
    local run_id="${1:-}"
    if [[ -z "${run_id}" ]]; then
        log_error "Usage: $0 stop <run_id>"
        exit 1
    fi

    check_systemd

    log_info "Stopping ${SERVICE_NAME}@${run_id}..."
    systemctl --user stop "${SERVICE_NAME}@${run_id}.service" || true
    log_info "Service stopped."
}

cmd_restart() {
    local run_id="${1:-}"
    local env_file="${2:-}"

    if [[ -z "${run_id}" ]]; then
        log_error "Usage: $0 restart <run_id> [env_file]"
        exit 1
    fi

    check_systemd

    # If env_file provided, regenerate start script
    if [[ -n "${env_file}" ]]; then
        generate_start_script "${run_id}" "${env_file}"
    fi

    log_info "Restarting ${SERVICE_NAME}@${run_id}..."
    systemctl --user restart "${SERVICE_NAME}@${run_id}.service"

    sleep 1
    if systemctl --user is-active --quiet "${SERVICE_NAME}@${run_id}.service"; then
        log_info "Service restarted successfully!"
    else
        log_error "Service failed to restart. Check logs:"
        journalctl --user -u "${SERVICE_NAME}@${run_id}.service" -n 20 --no-pager
    fi
}

cmd_status() {
    local run_id="${1:-}"
    if [[ -z "${run_id}" ]]; then
        log_error "Usage: $0 status <run_id>"
        exit 1
    fi

    check_systemd

    systemctl --user status "${SERVICE_NAME}@${run_id}.service" --no-pager || true
}

cmd_logs() {
    local run_id="${1:-}"
    local lines="${2:-50}"

    if [[ -z "${run_id}" ]]; then
        log_error "Usage: $0 logs <run_id> [num_lines]"
        exit 1
    fi

    check_systemd

    log_info "Showing logs for ${SERVICE_NAME}@${run_id} (Ctrl+C to exit)..."
    journalctl --user -u "${SERVICE_NAME}@${run_id}.service" -n "${lines}" -f
}

cmd_help() {
    cat << EOF
Psyche Daemon Management Script

Usage: $0 <command> [args]

Commands:
  install                       Install systemd service (run once)
  start <run_id> <env_file>     Start training client with given env file
  stop <run_id>                 Stop the training client
  restart <run_id> [env_file]   Restart (optionally with new env file)
  status <run_id>               Show service status
  logs <run_id> [lines]         Show and follow logs
  help                          Show this help

Examples:
  $0 install
  $0 start test config/client/.env.local
  $0 logs test
  $0 stop test
EOF
}

# Main
main() {
    local cmd="${1:-help}"
    shift || true

    case "${cmd}" in
        install) cmd_install "$@" ;;
        start) cmd_start "$@" ;;
        stop) cmd_stop "$@" ;;
        restart) cmd_restart "$@" ;;
        status) cmd_status "$@" ;;
        logs) cmd_logs "$@" ;;
        help|--help|-h) cmd_help ;;
        *)
            log_error "Unknown command: ${cmd}"
            cmd_help
            exit 1
            ;;
    esac
}

main "$@"
