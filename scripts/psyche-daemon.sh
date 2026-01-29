#!/usr/bin/env bash
#
# Psyche Daemon Management Script
# Runs psyche-solana-client binary as a systemd user service
#
# Usage:
#   psyche-daemon.sh install
#   psyche-daemon.sh start <run_id> --binary <path> --env <file> [--wallet <path>]
#   psyche-daemon.sh stop <run_id>
#   psyche-daemon.sh status <run_id>
#   psyche-daemon.sh logs <run_id>

set -eo pipefail

SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"
RUNTIME_DIR="${HOME}/.local/share/psyche"
SERVICE_NAME="psyche-client"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

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

cmd_install() {
    check_systemd

    log_info "Installing systemd service template..."

    mkdir -p "${SYSTEMD_USER_DIR}"
    mkdir -p "${RUNTIME_DIR}"

    cat > "${SYSTEMD_USER_DIR}/${SERVICE_NAME}@.service" << EOF
[Unit]
Description=Psyche Training Client (%i)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/bin/bash ${RUNTIME_DIR}/start-%i.sh
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
EOF

    systemctl --user daemon-reload
    log_info "Installed to ${SYSTEMD_USER_DIR}/${SERVICE_NAME}@.service"

    # Enable lingering
    if command -v loginctl &> /dev/null; then
        if ! loginctl show-user "$USER" 2>/dev/null | grep -q "Linger=yes"; then
            log_warn "Enabling lingering (services persist after logout)..."
            sudo loginctl enable-linger "$USER" 2>/dev/null || \
                log_warn "Could not enable lingering. Services may stop after logout."
        fi
    fi

    log_info "Done! Now use: $0 start <run_id> --binary <path> --env <file>"
}

cmd_start() {
    local run_id=""
    local binary=""
    local env_file=""
    local wallet=""

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --binary|-b) binary="$2"; shift 2 ;;
            --env|-e) env_file="$2"; shift 2 ;;
            --wallet|-w) wallet="$2"; shift 2 ;;
            -*) log_error "Unknown option: $1"; exit 1 ;;
            *) run_id="$1"; shift ;;
        esac
    done

    if [[ -z "${run_id}" ]] || [[ -z "${binary}" ]]; then
        log_error "Usage: $0 start <run_id> --binary <path> [--env <file>] [--wallet <path>]"
        echo ""
        echo "Examples:"
        echo "  $0 start test --binary /usr/local/bin/psyche-solana-client --env config.env"
        echo "  $0 start test --binary ./psyche-solana-client --wallet /path/to/wallet.json"
        echo "  $0 start test --binary /usr/local/bin/run-manager --env config.env"
        exit 1
    fi

    check_systemd

    # Resolve to absolute paths
    binary="$(cd "$(dirname "${binary}")" && pwd)/$(basename "${binary}")"
    if [[ ! -x "${binary}" ]]; then
        log_error "Binary not found or not executable: ${binary}"
        exit 1
    fi

    # Check if run-manager requires env file
    if [[ "$(basename "${binary}")" == "run-manager" ]] && [[ -z "${env_file}" ]]; then
        log_error "run-manager requires --env <file>"
        exit 1
    fi

    if [[ -n "${env_file}" ]]; then
        if [[ ! "${env_file}" = /* ]]; then
            env_file="$(pwd)/${env_file}"
        fi
        if [[ ! -f "${env_file}" ]]; then
            log_error "Env file not found: ${env_file}"
            exit 1
        fi
    fi

    if [[ -n "${wallet}" ]]; then
        if [[ ! "${wallet}" = /* ]]; then
            wallet="$(pwd)/${wallet}"
        fi
        if [[ ! -f "${wallet}" ]]; then
            log_error "Wallet file not found: ${wallet}"
            exit 1
        fi
    fi

    # Generate start script
    local start_script="${RUNTIME_DIR}/start-${run_id}.sh"
    mkdir -p "${RUNTIME_DIR}"

    cat > "${start_script}" << EOF
#!/bin/bash
set -eo pipefail

EOF

    # Add env file sourcing if provided
    if [[ -n "${env_file}" ]]; then
        cat >> "${start_script}" << EOF
# Load environment
set -a
source "${env_file}"
set +a

EOF
    fi

    cat >> "${start_script}" << EOF
export RUST_LOG="\${RUST_LOG:-info,psyche=debug}"

# Detect which binary we're running
BINARY_NAME="\$(basename "${binary}")"

if [[ "\${BINARY_NAME}" == "run-manager" ]]; then
    # run-manager mode: just pass --env-file
    exec "${binary}" --env-file "${env_file}"
else
    # psyche-solana-client mode: pass full train arguments
    WALLET="\${WALLET_PRIVATE_KEY_PATH:-\${devnet__keypair__wallet_PATH:-${wallet}}}"
    if [[ -z "\${WALLET}" ]]; then
        echo "ERROR: No wallet configured"
        exit 1
    fi

    exec "${binary}" train \\
        --wallet-private-key-path "\${WALLET}" \\
        --rpc "\${RPC:-http://127.0.0.1:8899}" \\
        --ws-rpc "\${WS_RPC:-ws://127.0.0.1:8900}" \\
        --run-id "${run_id}" \\
        --data-parallelism "\${DP:-1}" \\
        --tensor-parallelism "\${TP:-1}" \\
        --micro-batch-size "\${BATCH_SIZE:-1}" \\
        --authorizer "\${AUTHORIZER:-11111111111111111111111111111111}" \\
        --logs console
fi
EOF

    chmod +x "${start_script}"
    log_info "Generated ${start_script}"

    log_info "Starting ${SERVICE_NAME}@${run_id}..."
    systemctl --user start "${SERVICE_NAME}@${run_id}.service"

    sleep 1
    if systemctl --user is-active --quiet "${SERVICE_NAME}@${run_id}.service"; then
        log_info "Service started!"
        echo ""
        echo "  Logs:   $0 logs ${run_id}"
        echo "  Status: $0 status ${run_id}"
        echo "  Stop:   $0 stop ${run_id}"
    else
        log_error "Failed to start. Logs:"
        journalctl --user -u "${SERVICE_NAME}@${run_id}.service" -n 20 --no-pager
    fi
}

cmd_stop() {
    local run_id="${1:-}"
    [[ -z "${run_id}" ]] && { log_error "Usage: $0 stop <run_id>"; exit 1; }
    check_systemd
    log_info "Stopping ${SERVICE_NAME}@${run_id}..."
    systemctl --user stop "${SERVICE_NAME}@${run_id}.service" || true
    log_info "Stopped."
}

cmd_restart() {
    local run_id="${1:-}"
    [[ -z "${run_id}" ]] && { log_error "Usage: $0 restart <run_id>"; exit 1; }
    check_systemd
    log_info "Restarting ${SERVICE_NAME}@${run_id}..."
    systemctl --user restart "${SERVICE_NAME}@${run_id}.service"
}

cmd_status() {
    local run_id="${1:-}"
    [[ -z "${run_id}" ]] && { log_error "Usage: $0 status <run_id>"; exit 1; }
    check_systemd
    systemctl --user status "${SERVICE_NAME}@${run_id}.service" --no-pager || true
}

cmd_logs() {
    local run_id="${1:-}"
    local lines="${2:-50}"
    [[ -z "${run_id}" ]] && { log_error "Usage: $0 logs <run_id> [lines]"; exit 1; }
    check_systemd
    log_info "Logs for ${SERVICE_NAME}@${run_id} (Ctrl+C to exit)..."
    journalctl --user -u "${SERVICE_NAME}@${run_id}.service" -n "${lines}" -f
}

cmd_help() {
    cat << 'EOF'
Psyche Daemon - Run training client as a background service

Usage: psyche-daemon.sh <command> [args]

Commands:
  install                                       Install systemd service (once)
  start <run_id> --binary <path> [options]      Start training client
  stop <run_id>                                 Stop client
  restart <run_id>                              Restart client
  status <run_id>                               Show status
  logs <run_id> [lines]                         Follow logs

Start options:
  --binary, -b <path>    Path to binary (run-manager or psyche-solana-client)
  --env, -e <file>       Environment file (required for run-manager)
  --wallet, -w <path>    Wallet file path (for psyche-solana-client mode)

Examples:
  # Using run-manager (auto-manages Docker containers)
  psyche-daemon.sh install
  psyche-daemon.sh start myrun --binary /opt/psyche/run-manager --env config.env
  psyche-daemon.sh logs myrun

  # Using psyche-solana-client directly
  psyche-daemon.sh start myrun --binary /opt/psyche/psyche-solana-client --env config.env
  psyche-daemon.sh stop myrun

Binary modes:
  run-manager           Passes --env-file to run-manager
  psyche-solana-client  Passes full train command arguments

Environment variables (in env file for psyche-solana-client):
  WALLET_PRIVATE_KEY_PATH  Wallet JSON file path
  RPC                      Solana RPC URL (default: http://127.0.0.1:8899)
  WS_RPC                   Solana WebSocket URL (default: ws://127.0.0.1:8900)
  AUTHORIZER               Authorizer address
  DP                       Data parallelism (default: 1)
  TP                       Tensor parallelism (default: 1)
  BATCH_SIZE               Micro batch size (default: 1)
EOF
}

case "${1:-help}" in
    install) shift; cmd_install "$@" ;;
    start) shift; cmd_start "$@" ;;
    stop) shift; cmd_stop "$@" ;;
    restart) shift; cmd_restart "$@" ;;
    status) shift; cmd_status "$@" ;;
    logs) shift; cmd_logs "$@" ;;
    help|--help|-h) cmd_help ;;
    *) log_error "Unknown: $1"; cmd_help; exit 1 ;;
esac
